//! The typed settings registry.
//!
//! A [`SettingDef`] states a setting's id, type, default, description, scope
//! and whether changing it needs a restart. Every write is validated against
//! the definition by [`SettingType::validate`], so no layer can hold a value
//! of the wrong type.
//!
//! The registry is the single source for the Settings UI: [`Registry::schema`]
//! is a plain JSON document the frontend renders without knowing any setting.

use std::collections::BTreeMap;

use manang_kernel::{SettingKind, SettingScope, SettingSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value, json};

use crate::error::{SettingsError, ValidationError};

/// Longest string, path or list item accepted.
pub const MAX_STRING_BYTES: usize = 4096;
/// Most items accepted in a list.
pub const MAX_LIST_ITEMS: usize = 1024;

/// Which layers may hold a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Only the user file may set it.
    #[default]
    User,
    /// The user file sets the default and a project may override it.
    Project,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

/// The value type of a setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SettingType {
    Bool,
    Int {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    Float {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Enum {
        values: Vec<String>,
    },
    String,
    Path,
    List {
        item: Box<SettingType>,
    },
}

fn found(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "a table",
    }
}

fn wrong(path: &str, expected: &'static str, value: &Value) -> ValidationError {
    ValidationError::WrongType {
        path: path.to_owned(),
        expected,
        found: found(value),
    }
}

fn check_text(path: &str, s: &str, allow_empty: bool) -> Result<(), ValidationError> {
    if s.len() > MAX_STRING_BYTES {
        return Err(ValidationError::TooLong {
            path: path.to_owned(),
            len: s.len(),
            max: MAX_STRING_BYTES,
        });
    }
    if s.contains('\0') {
        return Err(ValidationError::Nul {
            path: path.to_owned(),
        });
    }
    if s.is_empty() && !allow_empty {
        return Err(ValidationError::Empty {
            path: path.to_owned(),
        });
    }
    Ok(())
}

impl SettingType {
    /// Short human description, e.g. `an integer from 8 to 32`.
    pub fn describe(&self) -> String {
        match self {
            Self::Bool => "a boolean".to_owned(),
            Self::Int { min, max } => range_text("an integer", min, max),
            Self::Float { min, max } => range_text("a number", min, max),
            Self::Enum { values } => format!("one of {}", values.join(", ")),
            Self::String => "a string".to_owned(),
            Self::Path => "a path string".to_owned(),
            Self::List { item } => format!("a list of {}", item.describe()),
        }
    }

    /// Check `value` and return its normalised form (floats become floats,
    /// everything else is unchanged). `path` is used in error messages.
    pub fn validate(&self, path: &str, value: &Value) -> Result<Value, ValidationError> {
        match self {
            Self::Bool => match value {
                Value::Bool(_) => Ok(value.clone()),
                other => Err(wrong(path, "a boolean", other)),
            },
            Self::Int { min, max } => {
                let Value::Number(n) = value else {
                    return Err(wrong(path, "an integer", value));
                };
                let Some(i) = n.as_i64() else {
                    if n.is_u64() {
                        return Err(ValidationError::AboveMax {
                            path: path.to_owned(),
                            value: n.to_string(),
                            max: max.map_or_else(|| i64::MAX.to_string(), |m| m.to_string()),
                        });
                    }
                    return Err(ValidationError::WrongType {
                        path: path.to_owned(),
                        expected: "an integer",
                        found: "a number with a fraction",
                    });
                };
                if let Some(min) = min.filter(|m| i < *m) {
                    return Err(ValidationError::BelowMin {
                        path: path.to_owned(),
                        value: i.to_string(),
                        min: min.to_string(),
                    });
                }
                if let Some(max) = max.filter(|m| i > *m) {
                    return Err(ValidationError::AboveMax {
                        path: path.to_owned(),
                        value: i.to_string(),
                        max: max.to_string(),
                    });
                }
                Ok(Value::Number(i.into()))
            }
            Self::Float { min, max } => {
                let Value::Number(n) = value else {
                    return Err(wrong(path, "a number", value));
                };
                let f = n.as_f64().filter(|f| f.is_finite()).ok_or_else(|| {
                    ValidationError::NotFinite {
                        path: path.to_owned(),
                    }
                })?;
                if let Some(min) = min.filter(|m| f < *m) {
                    return Err(ValidationError::BelowMin {
                        path: path.to_owned(),
                        value: f.to_string(),
                        min: min.to_string(),
                    });
                }
                if let Some(max) = max.filter(|m| f > *m) {
                    return Err(ValidationError::AboveMax {
                        path: path.to_owned(),
                        value: f.to_string(),
                        max: max.to_string(),
                    });
                }
                Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| ValidationError::NotFinite {
                        path: path.to_owned(),
                    })
            }
            Self::Enum { values } => match value {
                Value::String(s) if values.contains(s) => Ok(value.clone()),
                Value::String(s) => Err(ValidationError::NotInEnum {
                    path: path.to_owned(),
                    value: s.clone(),
                    allowed: values.join(", "),
                }),
                other => Err(wrong(path, "a string", other)),
            },
            Self::String => match value {
                Value::String(s) => check_text(path, s, true).map(|()| value.clone()),
                other => Err(wrong(path, "a string", other)),
            },
            Self::Path => match value {
                Value::String(s) => check_text(path, s, false).map(|()| value.clone()),
                other => Err(wrong(path, "a path string", other)),
            },
            Self::List { item } => {
                let Value::Array(items) = value else {
                    return Err(wrong(path, "an array", value));
                };
                if items.len() > MAX_LIST_ITEMS {
                    return Err(ValidationError::TooLong {
                        path: path.to_owned(),
                        len: items.len(),
                        max: MAX_LIST_ITEMS,
                    });
                }
                items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| item.validate(&format!("{path}[{i}]"), v))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::Array)
            }
        }
    }
}

fn range_text<T: std::fmt::Display>(noun: &str, min: &Option<T>, max: &Option<T>) -> String {
    match (min, max) {
        (Some(a), Some(b)) => format!("{noun} from {a} to {b}"),
        (Some(a), None) => format!("{noun} of at least {a}"),
        (None, Some(b)) => format!("{noun} of at most {b}"),
        (None, None) => noun.to_owned(),
    }
}

/// A declared setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SettingDef {
    /// Dotted lowercase kebab-case id such as `editor.font-size`.
    pub id: String,
    #[serde(flatten)]
    pub ty: SettingType,
    pub default: Value,
    pub description: String,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub requires_restart: bool,
    /// Short human label for the Settings UI; the UI falls back to the id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Who registered it (`core` or a plugin name). Set by the registry.
    #[serde(default, skip_deserializing)]
    pub owner: String,
    /// First id segment, for grouping in the UI. Set by the registry.
    #[serde(default, skip_deserializing)]
    pub category: String,
}

impl SettingDef {
    pub fn new(
        id: &str,
        ty: SettingType,
        default: Value,
        scope: Scope,
        requires_restart: bool,
        description: &str,
    ) -> Self {
        Self {
            id: id.to_owned(),
            ty,
            default,
            description: description.to_owned(),
            scope,
            requires_restart,
            title: None,
            owner: String::new(),
            category: String::new(),
        }
    }

    /// Set the human label shown by the Settings UI.
    #[must_use]
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_owned());
        self
    }

    /// Validate a candidate value for this setting.
    pub fn validate(&self, value: &Value) -> Result<Value, ValidationError> {
        self.ty.validate(&self.id, value)
    }
}

impl From<&SettingKind> for SettingType {
    fn from(kind: &SettingKind) -> Self {
        match kind {
            SettingKind::Bool => Self::Bool,
            SettingKind::Int { min, max } => Self::Int {
                min: *min,
                max: *max,
            },
            SettingKind::Float { min, max } => Self::Float {
                min: *min,
                max: *max,
            },
            SettingKind::Enum { values } => Self::Enum {
                values: values.clone(),
            },
            SettingKind::String => Self::String,
            SettingKind::Path => Self::Path,
            SettingKind::List { item } => Self::List {
                item: Box::new(Self::from(&**item)),
            },
        }
    }
}

impl From<&SettingSpec> for SettingDef {
    /// Convert a manifest declaration. A missing default (rejected by
    /// manifest validation) becomes `null`, which registration then refuses.
    fn from(spec: &SettingSpec) -> Self {
        Self {
            id: spec.id.clone(),
            ty: SettingType::from(&spec.kind),
            default: spec.default.clone().unwrap_or(Value::Null),
            description: spec.description.clone(),
            scope: match spec.scope {
                SettingScope::User => Scope::User,
                SettingScope::Project => Scope::Project,
            },
            requires_restart: spec.requires_restart,
            title: spec.title.clone(),
            owner: String::new(),
            category: String::new(),
        }
    }
}

/// True for `a.b-c.d`: at least two lowercase kebab-case segments.
pub(crate) fn valid_dotted_id(id: &str) -> bool {
    let mut parts = 0;
    for seg in id.split('.') {
        parts += 1;
        let mut chars = seg.chars();
        let ok = chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !seg.ends_with('-');
        if !ok {
            return false;
        }
    }
    parts >= 2
}

/// All registered settings, ordered by id.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    defs: BTreeMap<String, SettingDef>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry holding the shell's core settings.
    pub fn with_core() -> Self {
        let mut registry = Self::new();
        // Core definitions are covered by a test, so registration cannot fail.
        let _ = registry.register_all(crate::defaults::definitions(), "core");
        registry
    }

    pub fn get(&self, id: &str) -> Option<&SettingDef> {
        self.defs.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SettingDef> {
        self.defs.values()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    fn check(&self, def: &SettingDef, owner: &str) -> Result<bool, SettingsError> {
        let bad = |message: String| SettingsError::BadDefinition {
            id: def.id.clone(),
            message,
        };
        if !valid_dotted_id(&def.id) {
            return Err(SettingsError::BadId(def.id.clone()));
        }
        if def.description.trim().is_empty() {
            return Err(bad("a description is required".to_owned()));
        }
        match &def.ty {
            SettingType::Int {
                min: Some(a),
                max: Some(b),
            } if a > b => return Err(bad(format!("minimum {a} is above maximum {b}"))),
            SettingType::Float { min, max } => {
                if min.is_some_and(|m| !m.is_finite()) || max.is_some_and(|m| !m.is_finite()) {
                    return Err(bad("range bounds must be finite".to_owned()));
                }
                if let (Some(a), Some(b)) = (min, max)
                    && a > b
                {
                    return Err(bad(format!("minimum {a} is above maximum {b}")));
                }
            }
            SettingType::Enum { values } if values.is_empty() => {
                return Err(bad("an enum needs at least one value".to_owned()));
            }
            SettingType::List { item } if matches!(**item, SettingType::List { .. }) => {
                return Err(bad("lists cannot be nested".to_owned()));
            }
            _ => {}
        }
        def.validate(&def.default)
            .map_err(|e| bad(format!("the default is invalid: {e}")))?;
        if let Some(existing) = self.defs.get(&def.id) {
            let same = SettingDef {
                owner: String::new(),
                category: String::new(),
                ..existing.clone()
            } == SettingDef {
                owner: String::new(),
                category: String::new(),
                ..def.clone()
            };
            if existing.owner == owner {
                // Identical: no-op. Changed by its owner (a hot-reloaded
                // plugin's new manifest): replace the definition.
                return Ok(!same);
            }
            return Err(SettingsError::Duplicate {
                id: def.id.clone(),
                owner: existing.owner.clone(),
            });
        }
        for other in self.defs.keys() {
            let nested = other.starts_with(&format!("{}.", def.id))
                || def.id.starts_with(&format!("{other}."));
            if nested {
                return Err(bad(format!("overlaps the id of `{other}`")));
            }
        }
        Ok(true)
    }

    /// Register one setting. Re-registering an identical definition by the
    /// same owner is a no-op; a changed one by the same owner replaces it.
    pub fn register(&mut self, def: SettingDef, owner: &str) -> Result<(), SettingsError> {
        self.register_all(vec![def], owner).map(|_| ())
    }

    /// Register a batch atomically: on error nothing is added. Returns how
    /// many settings were new.
    pub fn register_all(
        &mut self,
        defs: Vec<SettingDef>,
        owner: &str,
    ) -> Result<usize, SettingsError> {
        let mut staged = self.clone();
        let mut added = 0;
        for mut def in defs {
            if staged.check(&def, owner)? {
                def.owner = owner.to_owned();
                def.category = def.id.split('.').next().unwrap_or_default().to_owned();
                staged.defs.insert(def.id.clone(), def);
                added += 1;
            }
        }
        *self = staged;
        Ok(added)
    }

    /// The full registry as JSON, for building a Settings UI.
    pub fn schema(&self) -> Value {
        let mut categories: Vec<&str> = self.defs.values().map(|d| d.category.as_str()).collect();
        categories.dedup();
        json!({
            "version": 1,
            "categories": categories,
            "settings": self.defs.values().collect::<Vec<_>>(),
        })
    }
}
