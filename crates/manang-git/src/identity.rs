//! The author identity (`user.name` / `user.email`): read where it comes
//! from, and set it for this repository or for every project.

use serde::Serialize;

use crate::ctx::Ctx;
use crate::error::GitError;
use crate::runner::Runner;

const MAX_NAME: usize = 200;
const MAX_EMAIL: usize = 254;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IdentitySource {
    /// Set in this repository's own config.
    Repo,
    /// Inherited from the user's global (or system) config.
    Global,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identity {
    pub name: Option<String>,
    pub email: Option<String>,
    pub source: IdentitySource,
}

/// One config value; `None` when unset or blank. Exit code 1 means unset.
fn value(run: &Runner, scope: Option<&str>, key: &str) -> Result<Option<String>, GitError> {
    let mut args = vec!["config"];
    args.extend(scope);
    args.extend(["--get", key]);
    let out = run.run(&args)?;
    if out.code == 1 {
        return Ok(None);
    }
    if !out.ok() {
        return Err(GitError::CommandFailed {
            command: "config".to_owned(),
            code: out.code,
            stderr: out.stderr,
        });
    }
    let text = out.stdout_text().trim().to_owned();
    Ok((!text.is_empty()).then_some(text))
}

pub fn get(ctx: &Ctx) -> Result<Identity, GitError> {
    let run = Runner::new(ctx.root.path());
    let name = value(&run, None, "user.name")?;
    let email = value(&run, None, "user.email")?;
    let local = value(&run, Some("--local"), "user.name")?.is_some()
        || value(&run, Some("--local"), "user.email")?.is_some();
    let source = if local {
        IdentitySource::Repo
    } else if name.is_some() || email.is_some() {
        IdentitySource::Global
    } else {
        IdentitySource::None
    };
    Ok(Identity {
        name,
        email,
        source,
    })
}

fn clean_name(name: &str) -> Result<&str, GitError> {
    // Control characters (a stray newline) are refused before trimming.
    if name.chars().any(char::is_control) {
        return Err(GitError::BadArgs(
            "the name must not contain line breaks or control characters".to_owned(),
        ));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(GitError::BadArgs("the name must not be empty".to_owned()));
    }
    if name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
        return Err(GitError::BadArgs(
            "the name is too long or contains control characters".to_owned(),
        ));
    }
    Ok(name)
}

fn clean_email(email: &str) -> Result<&str, GitError> {
    if email.chars().any(char::is_control) {
        return Err(GitError::BadArgs(
            "that does not look like an email address".to_owned(),
        ));
    }
    let email = email.trim();
    let bad = || GitError::BadArgs("that does not look like an email address".to_owned());
    if email.is_empty() || email.len() > MAX_EMAIL {
        return Err(bad());
    }
    if email
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '<' | '>'))
    {
        return Err(bad());
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err(bad());
    };
    if local.is_empty()
        || domain.contains('@')
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(bad());
    }
    Ok(email)
}

/// Write the identity to the repository config, or to the global config.
pub fn set(ctx: &Ctx, name: &str, email: &str, global: bool) -> Result<(), GitError> {
    let name = clean_name(name)?;
    let email = clean_email(email)?;
    let scope = if global { "--global" } else { "--local" };
    let run = Runner::new(ctx.root.path());
    run.run_ok(&["config", scope, "user.name", name])?;
    run.run_ok(&["config", scope, "user.email", email])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_emails_are_validated() {
        assert!(clean_name("Ada Lovelace").is_ok());
        assert!(clean_name("  ").is_err());
        assert!(clean_name("Ada\nLovelace").is_err());
        assert!(clean_name("Ada\u{7}").is_err());
        assert!(clean_email("ada@example.org").is_ok());
        for bad in [
            "",
            "ada",
            "ada@",
            "@example.org",
            "ada@example",
            "a b@example.org",
            "ada@example.org\n",
            "<ada@example.org>",
            "a@@example.org",
            "ada@.org",
        ] {
            assert!(clean_email(bad).is_err(), "{bad:?}");
        }
    }
}
