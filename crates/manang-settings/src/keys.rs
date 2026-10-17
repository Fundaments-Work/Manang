//! Shortcut strings: parsing, normalisation and platform mapping.
//!
//! Accepted forms: `Ctrl+Shift+P`, `cmd+k`, `Mod+S`, `Alt+ArrowUp`, `F11`
//! and chords with space-separated steps such as `Ctrl+K Ctrl+S`.
//!
//! Matching is case-insensitive and modifier order does not matter. The
//! canonical form orders modifiers `Mod, Ctrl, Alt, Shift, Meta` and writes
//! keys by name (`Escape`, `ArrowUp`, `Plus`) or as an uppercase letter.
//!
//! `Mod` is the platform's primary modifier: Cmd (`Meta`) on macOS, Ctrl
//! elsewhere. It stays abstract in stored and canonical text so one
//! `keybindings.toml` works across machines; [`Shortcut::concrete`] maps it
//! for the running platform.

use std::fmt;

use crate::error::KeyError;

/// Most steps a chord may have.
pub const MAX_CHORD_STEPS: usize = 4;

/// The operating system family, which decides what `Mod` means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mac => "mac",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}

/// A set of held modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
pub struct Mods {
    pub primary: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

/// One key press with modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Stroke {
    pub mods: Mods,
    /// Canonical key name.
    pub key: String,
}

/// A key press or a chord of several.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shortcut(pub Vec<Stroke>);

const NAMED_KEYS: &[(&str, &[&str])] = &[
    ("Enter", &["enter", "return"]),
    ("Escape", &["escape", "esc"]),
    ("Tab", &["tab"]),
    ("Space", &["space", "spacebar"]),
    ("Backspace", &["backspace"]),
    ("Delete", &["delete", "del"]),
    ("Insert", &["insert", "ins"]),
    ("Home", &["home"]),
    ("End", &["end"]),
    ("PageUp", &["pageup", "pgup"]),
    ("PageDown", &["pagedown", "pgdn", "pgdown"]),
    ("ArrowUp", &["arrowup", "up"]),
    ("ArrowDown", &["arrowdown", "down"]),
    ("ArrowLeft", &["arrowleft", "left"]),
    ("ArrowRight", &["arrowright", "right"]),
    ("Plus", &["plus"]),
    ("-", &["minus"]),
    (",", &["comma"]),
    (".", &["period"]),
    ("/", &["slash"]),
];

const PUNCTUATION: &str = ",./;'[]\\-=`";

fn modifier(token: &str) -> Option<fn(&mut Mods) -> &mut bool> {
    Some(match token {
        "mod" | "primary" => |m| &mut m.primary,
        "ctrl" | "control" => |m| &mut m.ctrl,
        "alt" | "option" | "opt" => |m| &mut m.alt,
        "shift" => |m| &mut m.shift,
        "meta" | "cmd" | "command" | "super" | "win" | "windows" => |m| &mut m.meta,
        _ => return None,
    })
}

fn canonical_key(shortcut: &str, token: &str) -> Result<String, KeyError> {
    let lower = token.to_lowercase();
    for (name, aliases) in NAMED_KEYS {
        if aliases.contains(&lower.as_str()) {
            return Ok((*name).to_owned());
        }
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
        && lower.len() > 1
    {
        return Ok(format!("F{n}"));
    }
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_digit() || PUNCTUATION.contains(c) {
            return Ok(c.to_string());
        }
        if c.is_alphabetic() {
            return Ok(c.to_uppercase().collect());
        }
    }
    Err(KeyError::UnknownKey {
        shortcut: shortcut.to_owned(),
        key: token.to_owned(),
    })
}

impl Stroke {
    fn parse(step: &str) -> Result<Self, KeyError> {
        let mut mods = Mods::default();
        let mut key: Option<String> = None;
        for token in step.split('+') {
            if token.is_empty() {
                return Err(KeyError::EmptyPart(step.to_owned()));
            }
            if let Some(slot) = modifier(&token.to_lowercase()) {
                let flag = slot(&mut mods);
                if *flag {
                    return Err(KeyError::DuplicateModifier {
                        shortcut: step.to_owned(),
                        modifier: token.to_owned(),
                    });
                }
                *flag = true;
            } else {
                let name = canonical_key(step, token)?;
                if let Some(first) = key.replace(name.clone()) {
                    return Err(KeyError::TwoKeys {
                        shortcut: step.to_owned(),
                        first,
                        second: name,
                    });
                }
            }
        }
        let key = key.ok_or_else(|| KeyError::NoKey {
            shortcut: step.to_owned(),
        })?;
        Ok(Self { mods, key })
    }

    fn concrete(&self, platform: Platform) -> Self {
        let mut mods = self.mods;
        if mods.primary {
            mods.primary = false;
            if platform == Platform::Mac {
                mods.meta = true;
            } else {
                mods.ctrl = true;
            }
        }
        Self {
            mods,
            key: self.key.clone(),
        }
    }
}

impl fmt::Display for Stroke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.mods;
        for (on, name) in [
            (m.primary, "Mod"),
            (m.ctrl, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.meta, "Meta"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, stroke) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{stroke}")?;
        }
        Ok(())
    }
}

impl Shortcut {
    pub fn parse(input: &str) -> Result<Self, KeyError> {
        let steps: Vec<&str> = input.split_whitespace().collect();
        if steps.is_empty() {
            return Err(KeyError::Empty);
        }
        if steps.len() > MAX_CHORD_STEPS {
            return Err(KeyError::ChordTooLong(input.to_owned(), MAX_CHORD_STEPS));
        }
        steps
            .into_iter()
            .map(Stroke::parse)
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    /// Map `Mod` to the platform's modifier.
    #[must_use]
    pub fn concrete(&self, platform: Platform) -> Self {
        Self(self.0.iter().map(|s| s.concrete(platform)).collect())
    }

    /// True when `self` is a strict prefix of `other` (a chord in progress).
    pub fn is_strict_prefix_of(&self, other: &Self) -> bool {
        self.0.len() < other.0.len() && other.0.starts_with(&self.0)
    }

    /// How the shortcut reads on `platform`: `Cmd+K` on macOS, `Ctrl+K`
    /// elsewhere.
    pub fn display(&self, platform: Platform) -> String {
        let concrete = self.concrete(platform).to_string();
        if platform == Platform::Mac {
            concrete.replace("Meta", "Cmd")
        } else {
            concrete
        }
    }
}

/// Parse and return the canonical form (`Mod` kept abstract).
pub fn normalize(input: &str) -> Result<String, KeyError> {
    Shortcut::parse(input).map(|s| s.to_string())
}

/// Parse and return the canonical form with `Mod` resolved for `platform`.
pub fn normalize_for(input: &str, platform: Platform) -> Result<String, KeyError> {
    Shortcut::parse(input).map(|s| s.concrete(platform).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_aliases_case_and_order() {
        assert_eq!(normalize("shift+ctrl+p").as_deref(), Ok("Ctrl+Shift+P"));
        assert_eq!(normalize("Cmd+K").as_deref(), Ok("Meta+K"));
        assert_eq!(normalize("option+up").as_deref(), Ok("Alt+ArrowUp"));
        assert_eq!(normalize("mod+,").as_deref(), Ok("Mod+,"));
        assert_eq!(normalize("ctrl+plus").as_deref(), Ok("Ctrl+Plus"));
        assert_eq!(normalize("f11").as_deref(), Ok("F11"));
        assert_eq!(normalize("ESC").as_deref(), Ok("Escape"));
        assert_eq!(normalize("ctrl+k   CTRL+s").as_deref(), Ok("Ctrl+K Ctrl+S"));
    }

    #[test]
    fn platform_mapping() {
        assert_eq!(
            normalize_for("Mod+S", Platform::Mac).as_deref(),
            Ok("Meta+S")
        );
        assert_eq!(
            normalize_for("Mod+S", Platform::Linux).as_deref(),
            Ok("Ctrl+S")
        );
        assert_eq!(
            Shortcut::parse("Mod+Shift+P").map(|s| s.display(Platform::Mac)),
            Ok("Shift+Cmd+P".to_owned())
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(normalize(""), Err(KeyError::Empty));
        assert!(matches!(normalize("Ctrl+"), Err(KeyError::EmptyPart(_))));
        assert!(matches!(normalize("Ctrl++"), Err(KeyError::EmptyPart(_))));
        assert!(matches!(
            normalize("Ctrl+Shift"),
            Err(KeyError::NoKey { .. })
        ));
        assert!(matches!(
            normalize("Ctrl+A+B"),
            Err(KeyError::TwoKeys { .. })
        ));
        assert!(matches!(
            normalize("Ctrl+Control+A"),
            Err(KeyError::DuplicateModifier { .. })
        ));
        assert!(matches!(
            normalize("Ctrl+F25"),
            Err(KeyError::UnknownKey { .. })
        ));
        assert!(matches!(
            normalize("A B C D E"),
            Err(KeyError::ChordTooLong(..))
        ));
    }

    #[test]
    fn chord_prefix() {
        let a = Shortcut::parse("Ctrl+K").expect("parse");
        let b = Shortcut::parse("Ctrl+K Ctrl+S").expect("parse");
        assert!(a.is_strict_prefix_of(&b));
        assert!(!b.is_strict_prefix_of(&a));
        assert!(!a.is_strict_prefix_of(&a));
    }
}
