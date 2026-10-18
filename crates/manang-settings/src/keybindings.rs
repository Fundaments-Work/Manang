//! Keybinding registry, user overrides, resolution and conflict detection.
//!
//! Precedence when several bindings match a key press in the active
//! contexts:
//!
//! 1. a user binding beats a default binding;
//! 2. among equals, the binding with more `when` clauses (the more specific
//!    one) wins;
//! 3. if two different commands are still tied, nothing is picked: the
//!    result is a [`Resolution::Conflict`] naming both.
//!
//! A user entry for a command replaces *all* of that command's defaults, so
//! rebinding never leaves the old key active. An entry with no keys unbinds.
//!
//! [`KeybindingSet::conflicts`] additionally reports user bindings that
//! shadow another command's default, because the displaced command silently
//! loses its shortcut otherwise.
//!
//! `keybindings.toml`:
//!
//! ```toml
//! [[bindings]]
//! command = "editor.save"
//! keys = ["Mod+S", "Ctrl+Alt+S"]
//! when = "editorFocus"   # optional; `a && !b`
//! ```

use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table};

use crate::error::{KeyError, SettingsError};
use crate::keys::{Platform, Shortcut};
use crate::registry::valid_dotted_id;

/// A `when` expression: a conjunction of context names, each optionally
/// negated. Empty means always active.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct When(Vec<(String, bool)>);

fn valid_context(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl When {
    pub fn parse(input: &str) -> Result<Self, KeyError> {
        let bad = || KeyError::BadWhen(input.to_owned());
        if input.trim().is_empty() {
            return Ok(Self::default());
        }
        let mut clauses: Vec<(String, bool)> = Vec::new();
        for part in input.split("&&") {
            let part = part.trim();
            let (name, positive) = match part.strip_prefix('!') {
                Some(rest) => (rest.trim(), false),
                None => (part, true),
            };
            if !valid_context(name) {
                return Err(bad());
            }
            clauses.push((name.to_owned(), positive));
        }
        clauses.sort();
        clauses.dedup();
        let contradictory = clauses.windows(2).any(|w| w[0].0 == w[1].0);
        if contradictory {
            return Err(bad());
        }
        Ok(Self(clauses))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The context names that must be present, for building a satisfying set.
    pub fn positive_contexts(&self) -> HashSet<String> {
        self.0
            .iter()
            .filter(|(_, p)| *p)
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// Number of clauses; more means more specific.
    pub fn specificity(&self) -> usize {
        self.0.len()
    }

    pub fn active(&self, contexts: &HashSet<String>) -> bool {
        self.0
            .iter()
            .all(|(name, positive)| contexts.contains(name) == *positive)
    }

    /// Whether both expressions can be true at once.
    pub fn compatible(&self, other: &Self) -> bool {
        !self
            .0
            .iter()
            .any(|(n, p)| other.0.iter().any(|(on, op)| on == n && op != p))
    }
}

impl std::fmt::Display for When {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, (name, positive)) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" && ")?;
            }
            if !positive {
                f.write_str("!")?;
            }
            f.write_str(name)?;
        }
        Ok(())
    }
}

impl Serialize for When {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BindSource {
    Default,
    User,
}

/// One effective binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub command: String,
    pub key: Shortcut,
    pub when: When,
    pub source: BindSource,
}

/// A default binding as a plugin declares it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultSpec {
    pub command: String,
    pub key: String,
    #[serde(default)]
    pub when: String,
}

/// A user override for one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEntry {
    pub command: String,
    pub keys: Vec<Shortcut>,
    pub when: When,
}

/// Outcome of resolving a key press.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Resolution {
    /// Exactly one command applies.
    Matched {
        command: String,
    },
    /// The press is the start of a longer chord; wait for more input.
    Pending,
    /// Several commands tie; none was picked.
    Conflict {
        commands: Vec<String>,
    },
    None,
}

/// Two bindings that cannot both have their way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// Canonical key as it applies on this platform.
    pub key: String,
    /// `conflict`: tied and unresolvable. `shadowed`: a user binding takes the
    /// key from another command's default.
    pub kind: &'static str,
    pub commands: Vec<String>,
}

/// Defaults plus user overrides for one platform.
#[derive(Debug, Clone)]
pub struct KeybindingSet {
    platform: Platform,
    defaults: Vec<Binding>,
    user: Vec<UserEntry>,
}

/// Defaults the shell ships, under the frontend's command ids
/// (`src/lib/commands/workbench.ts`). The frontend keeps a copy in
/// `src/lib/commands/defaults.ts` for the browser build; a vitest test reads
/// this table and fails if the two drift apart.
///
/// Keys handled by the focused surface itself (Undo, Copy, Find in the
/// editor, ...) and Escape, which dismisses whatever is on top, are not here:
/// they are not commands the shell dispatches.
pub const CORE_DEFAULTS: &[(&str, &str, &str)] = &[
    ("palette.commands", "Mod+K", ""),
    ("palette.commands", "Mod+Shift+P", ""),
    ("goto.file", "Mod+P", ""),
    ("project.switch", "Mod+O", ""),
    ("home.show", "Mod+Shift+H", ""),
    ("view.files", "Mod+Shift+E", ""),
    ("view.search", "Mod+Shift+F", ""),
    ("view.plugins", "Mod+Shift+X", ""),
    ("view.settings", "Mod+,", ""),
    ("view.toggle-side", "Mod+B", ""),
    ("view.toggle-panel", "Mod+J", ""),
    ("view.problems", "Mod+Shift+M", ""),
    ("view.cycle-preview", "Mod+Shift+V", ""),
    ("view.zen", "Mod+\\", ""),
    ("tab.next", "Mod+PageDown", ""),
    ("tab.next", "Mod+Tab", ""),
    ("tab.previous", "Mod+PageUp", ""),
    ("tab.previous", "Mod+Shift+Tab", ""),
    ("tab.close", "Mod+W", ""),
    ("tab.go-1", "Mod+1", ""),
    ("tab.go-2", "Mod+2", ""),
    ("tab.go-3", "Mod+3", ""),
    ("tab.go-4", "Mod+4", ""),
    ("tab.go-5", "Mod+5", ""),
    ("tab.go-6", "Mod+6", ""),
    ("tab.go-7", "Mod+7", ""),
    ("tab.go-8", "Mod+8", ""),
    ("tab.go-9", "Mod+9", ""),
    ("file.new", "Mod+N", ""),
    ("file.save", "Mod+S", ""),
    ("file.save-all", "Mod+Shift+S", ""),
    ("file.save-version", "Mod+Alt+S", ""),
    ("doc.compile", "Mod+Shift+B", ""),
    ("doc.export-pdf", "Mod+E", ""),
    ("doc.format", "Alt+Shift+F", ""),
    ("doc.show-in-preview", "Mod+Alt+J", ""),
    ("doc.present", "F5", ""),
    ("help.shortcuts", "Mod+/", ""),
    ("dev.inspect", "F12", ""),
    ("dev.inspect", "Mod+Shift+I", ""),
    ("app.quit", "Mod+Q", ""),
];

/// Command ids that earlier versions shipped, and the id each became. A user's
/// `keybindings.toml` written against an old id keeps working: it is read
/// (and rewritten, when changed from Settings) under the new id.
pub const COMMAND_ALIASES: &[(&str, &str)] = &[
    ("palette.open", "palette.commands"),
    ("library.toggle", "goto.file"),
    ("editor.save", "file.save"),
    ("editor.find", "edit.find"),
    ("project.open", "project.switch"),
    ("settings.open", "view.settings"),
    ("zen.toggle", "view.zen"),
    ("view.toggle-split", "view.cycle-preview"),
    ("compile.run", "doc.compile"),
];

/// The current id for `command`: itself, or what an old id became.
pub fn canonical_command(command: &str) -> &str {
    COMMAND_ALIASES
        .iter()
        .find(|(old, _)| *old == command)
        .map_or(command, |(_, new)| new)
}

pub fn valid_command(id: &str) -> Result<(), KeyError> {
    if valid_dotted_id(id) {
        Ok(())
    } else {
        Err(KeyError::BadCommand(id.to_owned()))
    }
}

impl KeybindingSet {
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            defaults: Vec::new(),
            user: Vec::new(),
        }
    }

    pub fn with_core(platform: Platform) -> Self {
        let mut set = Self::new(platform);
        let specs = CORE_DEFAULTS
            .iter()
            .map(|(c, k, w)| DefaultSpec {
                command: (*c).to_owned(),
                key: (*k).to_owned(),
                when: (*w).to_owned(),
            })
            .collect();
        // Core defaults are covered by a test, so this cannot fail.
        let _ = set.register(specs);
        set
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn user_entries(&self) -> &[UserEntry] {
        &self.user
    }

    /// Add default bindings atomically. Exact duplicates are ignored.
    pub fn register(&mut self, specs: Vec<DefaultSpec>) -> Result<usize, KeyError> {
        let mut staged = Vec::with_capacity(specs.len());
        for spec in specs {
            valid_command(&spec.command)?;
            staged.push(Binding {
                command: spec.command,
                key: Shortcut::parse(&spec.key)?,
                when: When::parse(&spec.when)?,
                source: BindSource::Default,
            });
        }
        let mut added = 0;
        for binding in staged {
            if !self.defaults.contains(&binding) {
                self.defaults.push(binding);
                added += 1;
            }
        }
        Ok(added)
    }

    pub fn set_user(&mut self, entries: Vec<UserEntry>) {
        self.user = entries;
    }

    /// Every binding in force, defaults first.
    pub fn bindings(&self) -> Vec<Binding> {
        let overridden: BTreeSet<&str> = self.user.iter().map(|e| e.command.as_str()).collect();
        let defaults = self
            .defaults
            .iter()
            .filter(|b| !overridden.contains(b.command.as_str()))
            .cloned();
        let user = self.user.iter().flat_map(|e| {
            e.keys.iter().map(|k| Binding {
                command: e.command.clone(),
                key: k.clone(),
                when: e.when.clone(),
                source: BindSource::User,
            })
        });
        defaults.chain(user).collect()
    }

    fn concrete(&self, b: &Binding) -> Shortcut {
        b.key.concrete(self.platform)
    }

    /// Decide what a key press (or chord so far) does in `contexts`.
    pub fn resolve(&self, contexts: &HashSet<String>, input: &str) -> Result<Resolution, KeyError> {
        let pressed = Shortcut::parse(input)?.concrete(self.platform);
        let active: Vec<(Binding, Shortcut)> = self
            .bindings()
            .into_iter()
            .filter(|b| b.when.active(contexts))
            .map(|b| {
                let key = self.concrete(&b);
                (b, key)
            })
            .collect();
        let exact: Vec<&Binding> = active
            .iter()
            .filter(|(_, key)| *key == pressed)
            .map(|(b, _)| b)
            .collect();
        if exact.is_empty() {
            let pending = active
                .iter()
                .any(|(_, key)| pressed.is_strict_prefix_of(key));
            return Ok(if pending {
                Resolution::Pending
            } else {
                Resolution::None
            });
        }
        let rank = |b: &Binding| (b.source == BindSource::User, b.when.specificity());
        let top = exact.iter().map(|b| rank(b)).max().unwrap_or_default();
        let commands: BTreeSet<&str> = exact
            .iter()
            .filter(|b| rank(b) == top)
            .map(|b| b.command.as_str())
            .collect();
        let mut commands: Vec<String> = commands.into_iter().map(str::to_owned).collect();
        Ok(if commands.len() == 1 {
            Resolution::Matched {
                command: commands.remove(0),
            }
        } else {
            Resolution::Conflict { commands }
        })
    }

    /// Bindings that collide on this platform. Deterministically ordered.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let all = self.bindings();
        let keyed: Vec<(&Binding, Shortcut)> = all.iter().map(|b| (b, self.concrete(b))).collect();
        let mut seen: BTreeSet<(String, &'static str, Vec<String>)> = BTreeSet::new();
        for (i, (a, ka)) in keyed.iter().enumerate() {
            for (b, kb) in &keyed[i + 1..] {
                if ka != kb || a.command == b.command || !a.when.compatible(&b.when) {
                    continue;
                }
                let kind = if a.source == b.source && a.when.specificity() == b.when.specificity() {
                    "conflict"
                } else if a.source != b.source {
                    "shadowed"
                } else {
                    continue;
                };
                let mut commands = vec![a.command.clone(), b.command.clone()];
                commands.sort();
                seen.insert((ka.display(self.platform), kind, commands));
            }
        }
        seen.into_iter()
            .map(|(key, kind, commands)| Conflict {
                key,
                kind,
                commands,
            })
            .collect()
    }

    /// The `keybindings.list` document.
    pub fn list_json(&self) -> Value {
        let bindings: Vec<Value> = self
            .bindings()
            .iter()
            .map(|b| {
                json!({
                    "command": b.command,
                    "key": b.key.to_string(),
                    "display": b.key.display(self.platform),
                    "when": b.when,
                    "source": b.source,
                })
            })
            .collect();
        json!({
            "platform": self.platform.as_str(),
            "bindings": bindings,
            "conflicts": self.conflicts(),
        })
    }
}

/// Read the user entries from the parsed `keybindings.toml`. Bad entries are
/// skipped and described in the returned warnings.
pub fn parse_user(doc: &Value) -> (Vec<UserEntry>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut entries = Vec::new();
    let Some(list) = doc.get("bindings") else {
        return (entries, warnings);
    };
    let Some(list) = list.as_array() else {
        warnings.push("`bindings` must be an array of tables ([[bindings]])".to_owned());
        return (entries, warnings);
    };
    for (n, raw) in list.iter().enumerate() {
        let at = format!("bindings[{n}]");
        match parse_entry(raw) {
            Ok(entry) => entries.push(entry),
            Err(message) => warnings.push(format!("{at}: {message}; entry ignored")),
        }
    }
    (entries, warnings)
}

fn parse_entry(raw: &Value) -> Result<UserEntry, String> {
    let command = raw
        .get("command")
        .and_then(Value::as_str)
        .ok_or("missing `command`")?;
    valid_command(command).map_err(|e| e.to_string())?;
    let mut keys = Vec::new();
    let raw_keys: Vec<&Value> = match (raw.get("keys"), raw.get("key")) {
        (Some(Value::Array(a)), _) => a.iter().collect(),
        (Some(_), _) => return Err("`keys` must be an array of strings".to_owned()),
        (None, Some(k)) => vec![k],
        (None, None) => Vec::new(),
    };
    for key in raw_keys {
        let text = key.as_str().ok_or("shortcuts must be strings")?;
        keys.push(Shortcut::parse(text).map_err(|e| e.to_string())?);
    }
    let when = match raw.get("when") {
        None => When::default(),
        Some(Value::String(w)) => When::parse(w).map_err(|e| e.to_string())?,
        Some(_) => return Err("`when` must be a string".to_owned()),
    };
    Ok(UserEntry {
        command: canonical_command(command).to_owned(),
        keys,
        when,
    })
}

fn bindings_table<'a>(
    doc: &'a mut DocumentMut,
    path: &str,
) -> Result<&'a mut ArrayOfTables, SettingsError> {
    let item = doc
        .as_table_mut()
        .entry("bindings")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    item.as_array_of_tables_mut()
        .ok_or_else(|| SettingsError::Unstorable {
            path: path.to_owned(),
            message: "`bindings` must be written as [[bindings]] tables".to_owned(),
        })
}

fn remove_command(aot: &mut ArrayOfTables, command: Option<&str>) -> bool {
    let mut removed = false;
    let mut i = aot.len();
    while i > 0 {
        i -= 1;
        // An entry under an old id counts as the command it became.
        let matches = command.is_none_or(|c| {
            aot.get(i)
                .and_then(|t| t.get("command"))
                .and_then(Item::as_str)
                .is_some_and(|stored| canonical_command(stored) == canonical_command(c))
        });
        if matches {
            aot.remove(i);
            removed = true;
        }
    }
    removed
}

/// Replace the override for `command` in `doc`.
pub fn write_override(
    doc: &mut DocumentMut,
    path: &str,
    command: &str,
    keys: &[Shortcut],
    when: &When,
) -> Result<bool, SettingsError> {
    let aot = bindings_table(doc, path)?;
    remove_command(aot, Some(command));
    let mut table = Table::new();
    table["command"] = toml_edit::value(canonical_command(command));
    let mut array = Array::new();
    for key in keys {
        array.push(key.to_string());
    }
    table["keys"] = toml_edit::value(array);
    if !when.is_empty() {
        table["when"] = toml_edit::value(when.to_string());
    }
    aot.push(table);
    Ok(true)
}

/// Drop the override for `command`, or every override when `None`.
pub fn remove_override(
    doc: &mut DocumentMut,
    path: &str,
    command: Option<&str>,
) -> Result<bool, SettingsError> {
    if doc.get("bindings").is_none() {
        return Ok(false);
    }
    let aot = bindings_table(doc, path)?;
    let removed = remove_command(aot, command);
    if aot.is_empty() {
        doc.as_table_mut().remove("bindings");
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn set() -> KeybindingSet {
        KeybindingSet::with_core(Platform::Linux)
    }

    fn entry(command: &str, keys: &[&str], when: &str) -> UserEntry {
        UserEntry {
            command: command.to_owned(),
            keys: keys
                .iter()
                .map(|k| Shortcut::parse(k).expect("key"))
                .collect(),
            when: When::parse(when).expect("when"),
        }
    }

    #[test]
    fn core_defaults_are_valid_and_conflict_free() {
        let s = set();
        assert_eq!(s.defaults.len(), CORE_DEFAULTS.len());
        assert_eq!(s.conflicts(), vec![]);
    }

    #[test]
    fn when_parsing() {
        assert_eq!(
            When::parse("b && !a").map(|w| w.to_string()),
            Ok("!a && b".to_owned())
        );
        assert!(When::parse("a && !a").is_err());
        assert!(When::parse("a ||| b").is_err());
        assert!(When::parse("").map(|w| w.is_empty()).unwrap_or(false));
        assert!(
            When::parse("a")
                .expect("when")
                .compatible(&When::parse("b").expect("when"))
        );
        assert!(
            !When::parse("a")
                .expect("when")
                .compatible(&When::parse("!a").expect("when"))
        );
    }

    #[test]
    fn resolves_by_context_and_platform() {
        let s = set();
        let got = s.resolve(&ctx(&[]), "ctrl+s");
        assert_eq!(
            got,
            Ok(Resolution::Matched {
                command: "file.save".to_owned()
            })
        );
        let mac = KeybindingSet::with_core(Platform::Mac);
        assert_eq!(
            mac.resolve(&ctx(&[]), "Cmd+S"),
            Ok(Resolution::Matched {
                command: "file.save".to_owned()
            })
        );
        assert_eq!(mac.resolve(&ctx(&[]), "Ctrl+S"), Ok(Resolution::None));
    }

    #[test]
    fn more_specific_when_wins() {
        let mut s = KeybindingSet::new(Platform::Linux);
        s.register(vec![
            DefaultSpec {
                command: "palette.close".to_owned(),
                key: "Escape".to_owned(),
                when: "paletteOpen".to_owned(),
            },
            DefaultSpec {
                command: "zen.exit".to_owned(),
                key: "Escape".to_owned(),
                when: "zenMode && !paletteOpen".to_owned(),
            },
        ])
        .expect("register");
        let got = s.resolve(&ctx(&["zenMode"]), "Escape");
        assert_eq!(
            got,
            Ok(Resolution::Matched {
                command: "zen.exit".to_owned()
            })
        );
        let got = s.resolve(&ctx(&["zenMode", "paletteOpen"]), "Escape");
        assert_eq!(
            got,
            Ok(Resolution::Matched {
                command: "palette.close".to_owned()
            })
        );
    }

    #[test]
    fn chords_are_pending_then_match() {
        let mut custom = KeybindingSet::new(Platform::Linux);
        custom
            .register(vec![DefaultSpec {
                command: "a.b".to_owned(),
                key: "Ctrl+X Ctrl+Y".to_owned(),
                when: String::new(),
            }])
            .expect("register");
        assert_eq!(custom.resolve(&ctx(&[]), "Ctrl+X"), Ok(Resolution::Pending));
        assert_eq!(
            custom.resolve(&ctx(&[]), "ctrl+x CTRL+y"),
            Ok(Resolution::Matched {
                command: "a.b".to_owned()
            })
        );
        assert_eq!(
            custom.resolve(&ctx(&[]), "Ctrl+X Ctrl+Z"),
            Ok(Resolution::None)
        );
    }

    #[test]
    fn user_override_replaces_defaults_and_reports_shadowing() {
        let mut s = set();
        s.set_user(vec![entry("doc.compile", &["Mod+S"], "")]);
        // The old key is gone.
        assert_eq!(s.resolve(&ctx(&[]), "Ctrl+Shift+B"), Ok(Resolution::None));
        // The user binding wins over file.save's default...
        assert_eq!(
            s.resolve(&ctx(&[]), "Ctrl+S"),
            Ok(Resolution::Matched {
                command: "doc.compile".to_owned()
            })
        );
        // ...but the displacement is reported.
        assert_eq!(
            s.conflicts(),
            vec![Conflict {
                key: "Ctrl+S".to_owned(),
                kind: "shadowed",
                commands: vec!["doc.compile".to_owned(), "file.save".to_owned()],
            }]
        );
    }

    #[test]
    fn ties_are_conflicts_not_picks() {
        let mut s = set();
        s.set_user(vec![
            entry("a.one", &["Ctrl+Alt+Y"], ""),
            entry("a.two", &["Ctrl+Alt+Y"], ""),
        ]);
        assert_eq!(
            s.resolve(&ctx(&[]), "Ctrl+Alt+Y"),
            Ok(Resolution::Conflict {
                commands: vec!["a.one".to_owned(), "a.two".to_owned()]
            })
        );
        assert_eq!(s.conflicts().len(), 1);
        assert_eq!(s.conflicts()[0].kind, "conflict");
    }

    #[test]
    fn unbinding_with_no_keys() {
        let mut s = set();
        s.set_user(vec![entry("project.switch", &[], "")]);
        assert_eq!(s.resolve(&ctx(&[]), "Ctrl+O"), Ok(Resolution::None));
    }

    #[test]
    fn user_file_round_trip_and_warnings() {
        let text = "[[bindings]]\ncommand = \"a.b\"\nkeys = [\"ctrl+j\"]\nwhen = \"x && !y\"\n\n[[bindings]]\ncommand = \"Bad\"\nkeys = []\n\n[[bindings]]\ncommand = \"a.c\"\nkeys = [\"Ctrl+Nope\"]\n";
        let doc: DocumentMut = text.parse().expect("toml");
        let (entries, warnings) = parse_user(&crate::store::doc_to_json(&doc));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].keys[0].to_string(), "Ctrl+J");
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn write_and_remove_overrides_keep_other_entries() {
        let mut doc: DocumentMut = "# mine\n[[bindings]]\ncommand = \"a.b\"\nkeys = [\"Ctrl+J\"]\n"
            .parse()
            .expect("toml");
        let key = Shortcut::parse("mod+shift+x").expect("key");
        write_override(&mut doc, "p", "a.c", &[key], &When::default()).expect("write");
        let (entries, _) = parse_user(&crate::store::doc_to_json(&doc));
        assert_eq!(entries.len(), 2);
        assert!(doc.to_string().starts_with("# mine\n"));
        assert!(remove_override(&mut doc, "p", Some("a.c")).expect("remove"));
        assert!(remove_override(&mut doc, "p", None).expect("remove"));
        assert!(doc.get("bindings").is_none());
        assert!(!remove_override(&mut doc, "p", None).expect("remove"));
    }

    #[test]
    fn core_defaults_use_frontend_ids_not_old_ones() {
        for (command, _, _) in CORE_DEFAULTS {
            assert_eq!(
                canonical_command(command),
                *command,
                "{command} is an old id"
            );
        }
        for (old, new) in COMMAND_ALIASES {
            assert_ne!(old, new);
            assert!(valid_command(new).is_ok(), "{new}");
            assert_eq!(canonical_command(new), *new, "aliases do not chain");
        }
        assert_eq!(canonical_command("view.files"), "view.files");
    }

    #[test]
    fn old_ids_in_the_user_file_are_read_as_the_new_ones() {
        let text = "[[bindings]]\ncommand = \"editor.save\"\nkeys = [\"Ctrl+Alt+S\"]\n\n[[bindings]]\ncommand = \"palette.open\"\nkeys = [\"F1\"]\n";
        let doc: DocumentMut = text.parse().expect("toml");
        let (entries, warnings) = parse_user(&crate::store::doc_to_json(&doc));
        assert!(warnings.is_empty(), "{warnings:?}");
        let commands: Vec<&str> = entries.iter().map(|e| e.command.as_str()).collect();
        assert_eq!(commands, vec!["file.save", "palette.commands"]);
        let mut s = set();
        s.set_user(entries);
        // The override replaces the default under the new id: Ctrl+S is free.
        assert_eq!(s.resolve(&ctx(&[]), "Ctrl+S"), Ok(Resolution::None));
        assert_eq!(
            s.resolve(&ctx(&[]), "Ctrl+Alt+S"),
            Ok(Resolution::Matched {
                command: "file.save".to_owned()
            })
        );
        assert_eq!(
            s.resolve(&ctx(&[]), "F1"),
            Ok(Resolution::Matched {
                command: "palette.commands".to_owned()
            })
        );
    }

    #[test]
    fn rewriting_an_old_id_replaces_it_under_the_new_one() {
        let mut doc: DocumentMut =
            "[[bindings]]\ncommand = \"compile.run\"\nkeys = [\"Ctrl+Enter\"]\n"
                .parse()
                .expect("toml");
        let key = Shortcut::parse("Mod+Shift+R").expect("key");
        write_override(&mut doc, "p", "doc.compile", &[key], &When::default()).expect("write");
        let text = doc.to_string();
        assert!(!text.contains("compile.run"), "{text}");
        assert_eq!(text.matches("[[bindings]]").count(), 1, "{text}");
        assert!(text.contains("command = \"doc.compile\""), "{text}");
        // Resetting by the new id removes an entry stored under the old one.
        let mut old: DocumentMut = "[[bindings]]\ncommand = \"zen.toggle\"\nkeys = []\n"
            .parse()
            .expect("toml");
        assert!(remove_override(&mut old, "p", Some("view.zen")).expect("remove"));
        assert!(old.get("bindings").is_none());
    }
}
