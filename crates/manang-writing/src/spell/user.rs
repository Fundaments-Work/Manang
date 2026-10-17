//! The user dictionary: a plain-text file, one word per line.
//!
//! The file lives at an injected path (the host chooses the config
//! directory), is loaded lazily, and is rewritten atomically (temp file, fsync,
//! rename) so a crash never leaves a truncated dictionary.

use std::collections::{BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use parking_lot::{RwLock, RwLockReadGuard};

use crate::error::WritingError;

const MAX_WORD_CHARS: usize = 100;

#[derive(Default)]
pub struct UserState {
    loaded: bool,
    words: BTreeSet<String>,
    /// Words that are already all-lowercase match in any casing.
    lowercase: HashSet<String>,
}

impl UserState {
    /// True when `word` was added by the user. A lowercase entry matches
    /// `word`, `Word` and `WORD`; a capitalised entry matches only itself.
    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(word)
            || (!self.lowercase.is_empty() && self.lowercase.contains(&word.to_lowercase()))
    }

    fn insert(&mut self, word: String) -> bool {
        if word.to_lowercase() == word {
            self.lowercase.insert(word.clone());
        }
        self.words.insert(word)
    }

    fn remove(&mut self, word: &str) -> bool {
        self.lowercase.remove(word);
        self.words.remove(word)
    }
}

pub struct UserDictionary {
    path: PathBuf,
    state: RwLock<UserState>,
}

fn valid(word: &str) -> Result<&str, WritingError> {
    let word = word.trim();
    if word.is_empty() {
        return Err(WritingError::bad_arg("word", "must not be empty"));
    }
    if word.chars().count() > MAX_WORD_CHARS {
        return Err(WritingError::bad_arg(
            "word",
            format!("longer than {MAX_WORD_CHARS} characters"),
        ));
    }
    if word.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(WritingError::bad_arg(
            "word",
            "must be a single word without whitespace",
        ));
    }
    if word.starts_with('#') {
        return Err(WritingError::bad_arg("word", "must not start with `#`"));
    }
    Ok(word)
}

impl UserDictionary {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: RwLock::new(UserState::default()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn ensure_loaded(&self) -> Result<(), WritingError> {
        if self.state.read().loaded {
            return Ok(());
        }
        let mut state = self.state.write();
        if state.loaded {
            return Ok(());
        }
        match fs::read_to_string(&self.path) {
            Ok(text) => {
                for line in text.lines() {
                    let line = line.trim();
                    if !line.is_empty() && !line.starts_with('#') {
                        state.insert(line.to_owned());
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(source) => {
                return Err(WritingError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        }
        state.loaded = true;
        Ok(())
    }

    /// Read access for a whole spell check, taken once.
    pub fn read(&self) -> Result<RwLockReadGuard<'_, UserState>, WritingError> {
        self.ensure_loaded()?;
        Ok(self.state.read())
    }

    pub fn words(&self) -> Result<Vec<String>, WritingError> {
        Ok(self.read()?.words.iter().cloned().collect())
    }

    /// Returns `true` when the word was new.
    pub fn add(&self, word: &str) -> Result<bool, WritingError> {
        let word = valid(word)?;
        self.ensure_loaded()?;
        let mut state = self.state.write();
        if state.words.contains(word) {
            return Ok(false);
        }
        state.insert(word.to_owned());
        if let Err(e) = self.persist(&state) {
            state.remove(word);
            return Err(e);
        }
        Ok(true)
    }

    /// Returns `true` when the word was present.
    pub fn remove(&self, word: &str) -> Result<bool, WritingError> {
        let word = valid(word)?;
        self.ensure_loaded()?;
        let mut state = self.state.write();
        if !state.remove(word) {
            return Ok(false);
        }
        if let Err(e) = self.persist(&state) {
            state.insert(word.to_owned());
            return Err(e);
        }
        Ok(true)
    }

    fn persist(&self, state: &UserState) -> Result<(), WritingError> {
        let io = |source| WritingError::Io {
            path: self.path.clone(),
            source,
        };
        let dir = match self.path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        fs::create_dir_all(dir).map_err(io)?;
        let name = self
            .path
            .file_name()
            .map_or_else(|| "user-dictionary".into(), |n| n.to_string_lossy());
        let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
        let mut body = String::new();
        for word in &state.words {
            body.push_str(word);
            body.push('\n');
        }
        let write = || -> std::io::Result<()> {
            let mut file = File::create(&tmp)?;
            file.write_all(body.as_bytes())?;
            file.sync_all()?;
            fs::rename(&tmp, &self.path)
        };
        write().map_err(|e| {
            // Best effort: never leave the temp file behind.
            let _ = fs::remove_file(&tmp);
            io(e)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> (tempfile::TempDir, UserDictionary) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cfg").join("user-dictionary.txt");
        (dir, UserDictionary::new(path))
    }

    #[test]
    fn add_remove_persist_and_reload() {
        let (_dir, d) = dict();
        assert!(d.words().expect("words").is_empty());
        assert!(d.add("Kubernetes").expect("add"));
        assert!(!d.add("Kubernetes").expect("dup"));
        assert!(d.add("tectonic").expect("add"));
        let text = std::fs::read_to_string(d.path()).expect("file");
        assert_eq!(text, "Kubernetes\ntectonic\n");

        let again = UserDictionary::new(d.path().to_path_buf());
        assert_eq!(again.words().expect("words"), ["Kubernetes", "tectonic"]);
        assert!(again.remove("tectonic").expect("remove"));
        assert!(!again.remove("tectonic").expect("gone"));
        assert_eq!(
            std::fs::read_to_string(d.path()).expect("file"),
            "Kubernetes\n"
        );
    }

    #[test]
    fn case_rules() {
        let (_dir, d) = dict();
        d.add("tectonic").expect("add");
        d.add("Kubernetes").expect("add");
        let s = d.read().expect("read");
        assert!(s.contains("tectonic") && s.contains("Tectonic") && s.contains("TECTONIC"));
        assert!(s.contains("Kubernetes"));
        assert!(!s.contains("kubernetes"));
    }

    #[test]
    fn rejects_bad_words_and_leaves_no_temp_files() {
        let (_dir, d) = dict();
        for bad in ["", "  ", "two words", "#x", "a\nb"] {
            assert!(d.add(bad).is_err(), "{bad:?}");
        }
        d.add("ok").expect("add");
        let leftovers: Vec<_> = std::fs::read_dir(d.path().parent().expect("parent"))
            .expect("dir")
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers.len(), 1, "{leftovers:?}");
    }

    #[test]
    fn comments_and_blank_lines_are_ignored_on_load() {
        let (_dir, d) = dict();
        std::fs::create_dir_all(d.path().parent().expect("parent")).expect("mkdir");
        std::fs::write(d.path(), "# note\n\nfoo\n  bar  \n").expect("write");
        assert_eq!(d.words().expect("words"), ["bar", "foo"]);
    }

    #[test]
    fn failed_write_rolls_back_memory() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The parent "directory" is a file, so persisting must fail.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").expect("file");
        let d = UserDictionary::new(blocker.join("words.txt"));
        assert!(d.add("word").is_err());
        assert!(d.words().expect("words").is_empty());
    }
}
