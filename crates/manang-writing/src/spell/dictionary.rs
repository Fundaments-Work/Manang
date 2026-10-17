//! Hunspell dictionary loading.
//!
//! Two sources, searched in this order:
//!
//! 1. the user's dictionary directory (`<config_dir>/dictionaries`): drop
//!    `xx_YY.aff` + `xx_YY.dic` (or `xx_YY/index.aff` + `xx_YY/index.dic`)
//!    there and the language becomes available, no rebuild needed. Files must
//!    be UTF-8 (`SET UTF-8`); convert legacy encodings with `iconv` first.
//! 2. the dictionaries compiled into the crate (gzip-compressed).

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::read::GzDecoder;
use parking_lot::RwLock;
use serde::Serialize;
pub use spellbook::Dictionary;

use crate::error::WritingError;

/// Language used when a request names none.
pub const DEFAULT_LANGUAGE: &str = "en_US";

struct Bundled {
    code: &'static str,
    aff: &'static [u8],
    dic: &'static [u8],
}

const BUNDLED: &[Bundled] = &[
    Bundled {
        code: "en_US",
        aff: include_bytes!("../../dict/en_US.aff.gz"),
        dic: include_bytes!("../../dict/en_US.dic.gz"),
    },
    Bundled {
        code: "en_GB",
        aff: include_bytes!("../../dict/en_GB.aff.gz"),
        dic: include_bytes!("../../dict/en_GB.dic.gz"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LanguageInfo {
    pub code: String,
    /// `bundled` or `user`.
    pub source: &'static str,
}

/// Canonical `xx_YY` form of a language tag (`en-gb`, `EN_gb`, `en_GB`).
pub fn normalise_language(tag: &str) -> String {
    let tag = tag.trim().replace('-', "_");
    let mut parts = tag.splitn(2, '_');
    let lang = parts.next().unwrap_or("").to_ascii_lowercase();
    match parts.next() {
        Some(region) if !region.is_empty() => format!("{lang}_{}", region.to_ascii_uppercase()),
        _ => lang,
    }
}

/// Lazily loaded, cached dictionaries.
pub struct Dictionaries {
    dir: PathBuf,
    cache: RwLock<HashMap<String, Arc<Dictionary>>>,
}

impl Dictionaries {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// The dictionary for `language` (`None` means [`DEFAULT_LANGUAGE`]).
    pub fn get(&self, language: Option<&str>) -> Result<(String, Arc<Dictionary>), WritingError> {
        let wanted = normalise_language(language.unwrap_or(DEFAULT_LANGUAGE));
        let code = self.resolve(&wanted)?;
        if let Some(hit) = self.cache.read().get(&code) {
            return Ok((code, Arc::clone(hit)));
        }
        let dictionary = Arc::new(self.load(&code)?);
        // A concurrent loader may have won; keep whichever landed first.
        let mut cache = self.cache.write();
        let entry = cache.entry(code.clone()).or_insert(dictionary);
        Ok((code, Arc::clone(entry)))
    }

    /// Map a requested tag to an available code: exact match first, then the
    /// bare language (`en`) to the first region we have for it.
    fn resolve(&self, wanted: &str) -> Result<String, WritingError> {
        let available = self.available();
        if available.iter().any(|l| l.code == wanted) {
            return Ok(wanted.to_owned());
        }
        if !wanted.contains('_') {
            let prefix = format!("{wanted}_");
            if wanted == "en" {
                return Ok(DEFAULT_LANGUAGE.to_owned());
            }
            if let Some(found) = available.iter().find(|l| l.code.starts_with(&prefix)) {
                return Ok(found.code.clone());
            }
        }
        Err(WritingError::NoDictionary(wanted.to_owned()))
    }

    /// Every language that can be loaded, user directory entries shadowing
    /// bundled ones.
    pub fn available(&self) -> Vec<LanguageInfo> {
        let mut map: BTreeMap<String, &'static str> = BTreeMap::new();
        for b in BUNDLED {
            map.insert(b.code.to_owned(), "bundled");
        }
        for code in self.user_codes() {
            map.insert(code, "user");
        }
        map.into_iter()
            .map(|(code, source)| LanguageInfo { code, source })
            .collect()
    }

    fn user_codes(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut codes = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if path.is_dir() {
                if path.join("index.aff").is_file() && path.join("index.dic").is_file() {
                    codes.push(normalise_language(name));
                }
            } else if let Some(stem) = name.strip_suffix(".dic")
                && self.dir.join(format!("{stem}.aff")).is_file()
            {
                codes.push(normalise_language(stem));
            }
        }
        codes
    }

    /// Locate the user's `.aff`/`.dic` pair for `code`, matching the file
    /// names case-insensitively so `en-gb.dic` and `en_GB.dic` both work.
    fn user_files(&self, code: &str) -> Option<(PathBuf, PathBuf)> {
        let entries = std::fs::read_dir(&self.dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_owned();
            if path.is_dir() {
                if normalise_language(&name) == code {
                    let (aff, dic) = (path.join("index.aff"), path.join("index.dic"));
                    if aff.is_file() && dic.is_file() {
                        return Some((aff, dic));
                    }
                }
            } else if let Some(stem) = name.strip_suffix(".dic")
                && normalise_language(stem) == code
            {
                let aff = self.dir.join(format!("{stem}.aff"));
                if aff.is_file() {
                    return Some((aff, path));
                }
            }
        }
        None
    }

    fn load(&self, code: &str) -> Result<Dictionary, WritingError> {
        let (aff, dic) = match self.user_files(code) {
            Some((aff, dic)) => (read_utf8(&aff, code)?, read_utf8(&dic, code)?),
            None => {
                let bundled = BUNDLED
                    .iter()
                    .find(|b| b.code == code)
                    .ok_or_else(|| WritingError::NoDictionary(code.to_owned()))?;
                (gunzip(bundled.aff, code)?, gunzip(bundled.dic, code)?)
            }
        };
        Dictionary::new(&aff, &dic).map_err(|e| WritingError::BadDictionary {
            language: code.to_owned(),
            reason: e.to_string(),
        })
    }
}

fn gunzip(data: &[u8], code: &str) -> Result<String, WritingError> {
    let mut out = String::new();
    GzDecoder::new(data)
        .read_to_string(&mut out)
        .map_err(|e| WritingError::BadDictionary {
            language: code.to_owned(),
            reason: format!("bundled data is corrupt: {e}"),
        })?;
    Ok(out)
}

fn read_utf8(path: &Path, code: &str) -> Result<String, WritingError> {
    let bytes = std::fs::read(path).map_err(|source| WritingError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    String::from_utf8(bytes).map_err(|_| WritingError::BadDictionary {
        language: code.to_owned(),
        reason: format!("`{}` is not UTF-8; convert it with iconv", path.display()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_tags() {
        assert_eq!(normalise_language("en-gb"), "en_GB");
        assert_eq!(normalise_language(" EN_us "), "en_US");
        assert_eq!(normalise_language("de"), "de");
    }

    #[test]
    fn loads_bundled_and_resolves_bare_language() {
        let dicts = Dictionaries::new(PathBuf::from("/nonexistent/dicts"));
        let (code, us) = dicts.get(None).expect("en_US loads");
        assert_eq!(code, "en_US");
        assert!(us.check("color"));
        assert!(!us.check("colour"));
        let (code, gb) = dicts.get(Some("en-gb")).expect("en_GB loads");
        assert_eq!(code, "en_GB");
        assert!(gb.check("colour"));
        assert_eq!(dicts.get(Some("en")).expect("alias").0, "en_US");
        assert!(matches!(
            dicts.get(Some("tlh")),
            Err(WritingError::NoDictionary(_))
        ));
    }

    #[test]
    fn user_directory_languages_are_discovered() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("xx_YY.aff"), "SET UTF-8\n").expect("aff");
        std::fs::write(dir.path().join("xx_YY.dic"), "2\nfoo\nbar\n").expect("dic");
        let dicts = Dictionaries::new(dir.path().to_path_buf());
        let langs: Vec<_> = dicts.available().into_iter().map(|l| l.code).collect();
        assert_eq!(langs, ["en_GB", "en_US", "xx_YY"]);
        let (_, xx) = dicts.get(Some("xx-yy")).expect("loads");
        assert!(xx.check("foo"));
        assert!(!xx.check("baz"));
        assert_eq!(dicts.get(Some("xx")).expect("bare").0, "xx_YY");
    }

    #[test]
    fn invalid_user_dictionary_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("zz.aff"), [0xff, 0xfe]).expect("aff");
        std::fs::write(dir.path().join("zz.dic"), "1\nx\n").expect("dic");
        let dicts = Dictionaries::new(dir.path().to_path_buf());
        assert!(matches!(
            dicts.get(Some("zz")),
            Err(WritingError::BadDictionary { .. })
        ));
    }
}
