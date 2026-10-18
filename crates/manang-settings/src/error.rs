//! Structured errors for the settings plugin.

use thiserror::Error;

/// A value does not satisfy a setting's declared type. `path` names the
/// setting, with `[n]` suffixes for list items (`files.exclude[2]`).
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ValidationError {
    #[error("`{path}` must be {expected}, found {found}")]
    WrongType {
        path: String,
        expected: &'static str,
        found: &'static str,
    },
    #[error("`{path}` is {value}, below the minimum {min}")]
    BelowMin {
        path: String,
        value: String,
        min: String,
    },
    #[error("`{path}` is {value}, above the maximum {max}")]
    AboveMax {
        path: String,
        value: String,
        max: String,
    },
    #[error("`{path}` must be one of {allowed}, found `{value}`")]
    NotInEnum {
        path: String,
        value: String,
        allowed: String,
    },
    #[error("`{path}` is {len} bytes long, above the limit of {max}")]
    TooLong {
        path: String,
        len: usize,
        max: usize,
    },
    #[error("`{path}` must not be empty")]
    Empty { path: String },
    #[error("`{path}` contains a NUL byte")]
    Nul { path: String },
    #[error("`{path}` must be a finite number")]
    NotFinite { path: String },
}

/// A shortcut string could not be understood.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KeyError {
    #[error("shortcut is empty")]
    Empty,
    #[error("shortcut `{0}` has an empty part; write the plus key as `Plus`")]
    EmptyPart(String),
    #[error("shortcut `{shortcut}` has no key, only modifiers")]
    NoKey { shortcut: String },
    #[error("shortcut `{shortcut}` has more than one key (`{first}` and `{second}`)")]
    TwoKeys {
        shortcut: String,
        first: String,
        second: String,
    },
    #[error("shortcut `{shortcut}` repeats the modifier `{modifier}`")]
    DuplicateModifier { shortcut: String, modifier: String },
    #[error("shortcut `{shortcut}` has an unknown key `{key}`")]
    UnknownKey { shortcut: String, key: String },
    #[error("chord `{0}` has more than {1} steps")]
    ChordTooLong(String, usize),
    #[error(
        "`when` clause `{0}` is invalid; use identifiers joined by `&&`, optionally negated with `!`"
    )]
    BadWhen(String),
    #[error("command id `{0}` must be dotted lowercase kebab-case such as `editor.save`")]
    BadCommand(String),
}

/// Anything that can go wrong in the settings plugin.
#[derive(Debug, Error)]
pub enum SettingsError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error("no setting `{0}` is registered")]
    UnknownSetting(String),
    #[error("setting id `{0}` must be dotted lowercase kebab-case with at least two parts")]
    BadId(String),
    #[error("setting `{id}`: {message}")]
    BadDefinition { id: String, message: String },
    #[error("setting `{id}` is already registered by `{owner}`")]
    Duplicate { id: String, owner: String },
    #[error("setting `{id}` is user-scoped and cannot be set in a project")]
    NotProjectScoped { id: String },
    #[error("no project is selected; call settings.use-project first")]
    NoProject,
    #[error("`{path}` is not valid TOML and was left untouched: {message}")]
    FileInvalid { path: String, message: String },
    #[error(transparent)]
    Format(#[from] manang_formats::FormatError),
    #[error("`{path}` cannot store this value: {message}")]
    Unstorable { path: String, message: String },
    #[error("no theme `{0}`")]
    UnknownTheme(String),
    #[error("theme `{id}` is invalid: {message}")]
    BadTheme { id: String, message: String },
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("file watching is unavailable: {0}")]
    Watch(String),
}

impl SettingsError {
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
