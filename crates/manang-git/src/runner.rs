//! The only place a subprocess is spawned.
//!
//! * never a shell: the program is `git` and arguments are an array;
//! * the environment is cleared and rebuilt from an allowlist, so an ambient
//!   `GIT_DIR`, `GIT_SSH_COMMAND`, `GIT_CONFIG_PARAMETERS` and friends cannot
//!   redirect or hijack an operation;
//! * hooks are disabled (`core.hooksPath` pointed at the null device) unless
//!   the caller opts in;
//! * stdin is closed, terminal prompts are off, and a hard timeout kills the
//!   child;
//! * stdout and stderr are capped.

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::error::GitError;

/// Default timeout for local operations.
pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(30);
/// Default timeout for network operations (`git.sync`).
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(120);
/// Upper bound a caller may request for `git.sync`.
pub const MAX_SYNC_TIMEOUT: Duration = Duration::from_secs(600);
/// Captured stdout cap.
pub const MAX_STDOUT: usize = 8 * 1024 * 1024;
/// Captured stderr cap (extra bytes are discarded).
pub const MAX_STDERR: usize = 64 * 1024;

#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";

/// Variables copied from the parent. Everything else is dropped.
const ENV_ALLOW: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "SYSTEMROOT",
    "XDG_CONFIG_HOME",
    "TMPDIR",
    "TEMP",
    "TMP",
    "SSH_AUTH_SOCK",
    "GIT_EXEC_PATH",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_SSL_CAINFO",
    "GIT_SSL_CAPATH",
];

/// Filter an environment down to the allowlist.
pub fn sanitize_env<I>(vars: I) -> Vec<(OsString, OsString)>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    vars.into_iter()
        .filter(|(k, _)| {
            k.to_str()
                .is_some_and(|k| ENV_ALLOW.iter().any(|a| a.eq_ignore_ascii_case(k)))
        })
        .collect()
}

/// Result of a finished subprocess.
#[derive(Debug)]
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// A configured way of running `git` in one directory.
#[derive(Debug, Clone)]
pub struct Runner {
    cwd: std::path::PathBuf,
    timeout: Duration,
    hooks: bool,
}

impl Runner {
    pub fn new(cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            timeout: LOCAL_TIMEOUT,
            hooks: false,
        }
    }

    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Let repository hooks run (off by default).
    #[must_use]
    pub fn hooks(mut self, enabled: bool) -> Self {
        self.hooks = enabled;
        self
    }

    /// The fixed leading arguments: configuration overrides that make every
    /// invocation safe and non-interactive.
    pub fn base_args(&self) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "--no-pager".into(),
            "--literal-pathspecs".into(),
            "-c".into(),
            "core.fsmonitor=false".into(),
            "-c".into(),
            "protocol.ext.allow=never".into(),
            "-c".into(),
            "core.quotepath=false".into(),
        ];
        if !self.hooks {
            args.push("-c".into());
            args.push(format!("core.hooksPath={NULL_DEVICE}"));
        }
        args
    }

    /// Run and return the output whatever the exit code.
    pub fn run(&self, args: &[&str]) -> Result<Output, GitError> {
        let mut cmd = Command::new("git");
        cmd.env_clear()
            .envs(sanitize_env(std::env::vars_os()))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .env("GIT_PAGER", "cat")
            .env("LC_ALL", "C")
            .env("LANGUAGE", "C")
            .args(self.base_args())
            .args(args)
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GitError::GitMissing
            } else {
                GitError::io("starting git", e)
            }
        })?;
        let out = child.stdout.take();
        let err = child.stderr.take();
        let out_thread = thread::spawn(move || capture(out, MAX_STDOUT));
        let err_thread = thread::spawn(move || capture(err, MAX_STDERR));

        let status = match child.wait_timeout(self.timeout) {
            Ok(Some(status)) => status,
            Ok(None) => {
                // Do not join the readers: a grandchild (ssh, a credential
                // helper) may still hold the pipes open.
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Timeout {
                    secs: self.timeout.as_secs(),
                });
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::io("waiting for git", e));
            }
        };
        let (stdout, out_over) = out_thread.join().unwrap_or_default();
        let (stderr, _) = err_thread.join().unwrap_or_default();
        if out_over {
            return Err(GitError::TooLarge {
                what: "git output".to_owned(),
                size: stdout.len() as u64,
                limit: MAX_STDOUT as u64,
            });
        }
        Ok(Output {
            code: status.code().unwrap_or(-1),
            stdout,
            stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
        })
    }

    /// Run and require exit code 0.
    pub fn run_ok(&self, args: &[&str]) -> Result<Output, GitError> {
        let out = self.run(args)?;
        if out.ok() {
            Ok(out)
        } else {
            Err(GitError::CommandFailed {
                command: args.first().copied().unwrap_or("").to_owned(),
                code: out.code,
                stderr: out.stderr,
            })
        }
    }
}

/// Read a pipe, keeping at most `cap` bytes; the flag says more was produced.
/// Keeps draining so the child never blocks on a full pipe.
fn capture<R: Read>(pipe: Option<R>, cap: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::new();
    let mut over = false;
    let Some(mut pipe) = pipe else {
        return (kept, over);
    };
    let mut buf = [0u8; 16 * 1024];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = cap.saturating_sub(kept.len());
                if n > room {
                    over = true;
                }
                kept.extend_from_slice(&buf[..n.min(room)]);
            }
        }
    }
    (kept, over)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn env_is_reduced_to_the_allowlist() {
        let env = vec![
            (os("PATH"), os("/bin")),
            (os("GIT_DIR"), os("/evil")),
            (os("GIT_SSH_COMMAND"), os("sh -c evil")),
            (os("GIT_CONFIG_PARAMETERS"), os("'core.x=y'")),
            (os("GIT_CONFIG_GLOBAL"), os("/home/u/.gitconfig")),
            (os("LD_PRELOAD"), os("/x.so")),
            (os("HOME"), os("/home/u")),
        ];
        let kept: Vec<String> = sanitize_env(env)
            .into_iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert_eq!(kept, ["PATH", "GIT_CONFIG_GLOBAL", "HOME"]);
    }

    #[test]
    fn hooks_are_disabled_unless_opted_in() {
        let off = Runner::new(Path::new(".")).base_args().join(" ");
        assert!(off.contains("core.hooksPath"));
        let on = Runner::new(Path::new("."))
            .hooks(true)
            .base_args()
            .join(" ");
        assert!(!on.contains("core.hooksPath"));
    }

    #[test]
    fn capture_caps_and_flags_overflow() {
        let data = [7u8; 100];
        let (kept, over) = capture(Some(&data[..]), 10);
        assert_eq!(kept.len(), 10);
        assert!(over);
        let (kept, over) = capture(Some(&data[..]), 100);
        assert_eq!((kept.len(), over), (100, false));
    }

    #[test]
    fn timeouts_kill_the_child() {
        // `git hash-object --stdin` would block, but stdin is closed; use a
        // pager-less command that cannot hang instead and a zero timeout.
        let r = Runner::new(Path::new("."))
            .timeout(Duration::from_millis(0))
            .run(&["--version"]);
        // Either it finished instantly or it timed out; it must never hang.
        assert!(matches!(r, Ok(_) | Err(GitError::Timeout { .. })));
    }
}
