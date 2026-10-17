//! Layered resolution: defaults < user file < project file.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::registry::{Registry, Scope, SettingDef};

/// Which layer a value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Default,
    User,
    Project,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

/// An effective value and the layer that supplied it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Resolved {
    pub value: Value,
    pub source: Source,
}

/// A problem found in a layer file. The offending entry is ignored (or, for
/// unknown keys, merely kept) and the app carries on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub layer: Source,
    pub id: Option<String>,
    pub message: String,
}

pub type Values = BTreeMap<String, Value>;

/// The value a layer supplies for `def`, if it is usable there.
fn usable(def: &SettingDef, layer: Source, values: &Values) -> Option<Value> {
    if layer == Source::Project && def.scope != Scope::Project {
        return None;
    }
    values.get(&def.id).and_then(|v| def.validate(v).ok())
}

/// Resolve one setting. A layer's value counts only if it validates, so a
/// bad hand edit falls through to the next layer instead of breaking the app.
pub fn resolve(def: &SettingDef, user: &Values, project: &Values) -> Resolved {
    if let Some(value) = usable(def, Source::Project, project) {
        return Resolved {
            value,
            source: Source::Project,
        };
    }
    if let Some(value) = usable(def, Source::User, user) {
        return Resolved {
            value,
            source: Source::User,
        };
    }
    Resolved {
        value: def.default.clone(),
        source: Source::Default,
    }
}

/// Resolve every registered setting.
pub fn resolve_all(
    registry: &Registry,
    user: &Values,
    project: &Values,
) -> BTreeMap<String, Resolved> {
    registry
        .iter()
        .map(|def| (def.id.clone(), resolve(def, user, project)))
        .collect()
}

/// Report everything wrong with one layer's raw values.
pub fn audit(registry: &Registry, layer: Source, values: &Values) -> Vec<Warning> {
    let warn = |id: &str, message: String| Warning {
        layer,
        id: Some(id.to_owned()),
        message,
    };
    values
        .iter()
        .filter_map(|(id, value)| match registry.get(id) {
            None => Some(warn(
                id,
                format!("unknown setting `{id}`; kept in the file but has no effect"),
            )),
            Some(def) if layer == Source::Project && def.scope != Scope::Project => Some(warn(
                id,
                format!("`{id}` is user-scoped and is ignored in a project"),
            )),
            Some(def) => def
                .validate(value)
                .err()
                .map(|e| warn(id, format!("{e}; ignored, using the next layer"))),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn values(pairs: &[(&str, Value)]) -> Values {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    #[test]
    fn project_beats_user_beats_default() {
        let reg = Registry::with_core();
        let def = reg.get("editor.tab-size").expect("core setting");
        let user = values(&[("editor.tab-size", json!(4))]);
        let project = values(&[("editor.tab-size", json!(8))]);
        assert_eq!(resolve(def, &user, &project).value, json!(8));
        assert_eq!(resolve(def, &user, &Values::new()).source, Source::User);
        assert_eq!(
            resolve(def, &Values::new(), &Values::new()).source,
            Source::Default
        );
    }

    #[test]
    fn invalid_values_fall_through() {
        let reg = Registry::with_core();
        let def = reg.get("editor.tab-size").expect("core setting");
        let user = values(&[("editor.tab-size", json!(4))]);
        let project = values(&[("editor.tab-size", json!(999))]);
        let got = resolve(def, &user, &project);
        assert_eq!((got.value, got.source), (json!(4), Source::User));
    }

    #[test]
    fn user_scoped_settings_ignore_the_project_layer() {
        let reg = Registry::with_core();
        let def = reg.get("files.autosave").expect("core setting");
        let project = values(&[("files.autosave", json!("off"))]);
        let got = resolve(def, &Values::new(), &project);
        assert_eq!(got.source, Source::Default);
        let warnings = audit(&reg, Source::Project, &project);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn audit_reports_unknown_and_invalid() {
        let reg = Registry::with_core();
        let user = values(&[
            ("editor.tab-size", json!("wide")),
            ("zzz.mystery", json!(1)),
            ("editor.word-wrap", json!(false)),
        ]);
        let ids: Vec<_> = audit(&reg, Source::User, &user)
            .into_iter()
            .filter_map(|w| w.id)
            .collect();
        assert_eq!(ids, vec!["editor.tab-size", "zzz.mystery"]);
    }
}
