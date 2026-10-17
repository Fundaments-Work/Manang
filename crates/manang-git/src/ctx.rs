//! A resolved repository context: the confined project root plus the open
//! `gix` repository containing it.

use std::path::{Path, PathBuf};

use manang_project::{ProjectRoot, lexical_relative, to_slash};

use crate::error::GitError;

/// Everything a command needs to act on one project.
pub struct Ctx {
    pub root: ProjectRoot,
    pub repo: gix::Repository,
    /// Project offset inside the work tree, `/`-separated with a trailing
    /// `/`, or empty when the project is the repository root.
    pub prefix: String,
}

fn not_a_repo(root: &ProjectRoot) -> GitError {
    GitError::NotARepo(root.path().display().to_string())
}

/// `true` if `dir` itself holds a `.git` entry (the project is a repo root).
pub fn has_dot_git(dir: &Path) -> bool {
    dir.join(".git").symlink_metadata().is_ok()
}

impl Ctx {
    /// Discover the repository above `root`. Fails with
    /// [`GitError::NotARepo`] for a plain directory or a bare repository.
    pub fn open(root: &ProjectRoot) -> Result<Self, GitError> {
        let repo = gix::discover(root.path()).map_err(|_| not_a_repo(root))?;
        let workdir = repo
            .workdir()
            .and_then(|w| w.canonicalize().ok())
            .ok_or_else(|| not_a_repo(root))?;
        let rel = root
            .path()
            .strip_prefix(&workdir)
            .map_err(|_| not_a_repo(root))?;
        let mut prefix = to_slash(rel).ok_or_else(|| not_a_repo(root))?;
        if !prefix.is_empty() {
            prefix.push('/');
        }
        Ok(Self {
            root: root.clone(),
            repo,
            prefix,
        })
    }

    /// The `.git` directory.
    pub fn git_dir(&self) -> PathBuf {
        self.repo.git_dir().to_path_buf()
    }

    /// Validate a caller path for reading: plain relative, no `..`, no
    /// symlink escape. Returns the normalised project-relative form.
    pub fn rel(&self, raw: &str) -> Result<String, GitError> {
        let lexical = lexical_relative(raw)?;
        self.root.resolve(raw)?;
        to_slash(&lexical).ok_or_else(|| {
            GitError::BadArgs(format!("path `{}` is not valid UTF-8", lexical.display()))
        })
    }

    /// Like [`rel`](Self::rel) for paths a write will touch: also refuses the
    /// root itself and anything inside `.git`.
    pub fn rel_for_write(&self, raw: &str) -> Result<String, GitError> {
        self.root.resolve_for_write(raw)?;
        self.rel(raw)
    }

    /// Repository-relative form of a project-relative path.
    pub fn repo_path(&self, rel: &str) -> String {
        format!("{}{rel}", self.prefix)
    }
}
