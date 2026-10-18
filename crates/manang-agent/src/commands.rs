//! Slash commands: prompt templates instead of per-feature system prompts.
//!
//! A command is Markdown: an optional one-line `description:` front matter and a body that becomes
//! the author's message. `$ARGUMENTS` is what the author typed after the name. The selection and
//! the active file arrive as the usual turn context, so a command carries only words.
//!
//! Built-ins are embedded strings. Files in `<project>/.manang/commands/*.md` and
//! `<config>/commands/*.md` override a built-in of the same name; the project wins.

use serde::Serialize;
use std::path::Path;

/// Where a command came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Builtin,
    User,
    Project,
}

/// One slash command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    /// Shown to the author as a hint after the name (`<language>`).
    pub hint: String,
    #[serde(skip)]
    pub body: String,
    pub source: Source,
}

/// Most files read from a command folder, and most characters of one command.
const MAX_FILES: usize = 64;
const MAX_BODY: usize = 8_000;

struct Builtin {
    name: &'static str,
    description: &'static str,
    hint: &'static str,
    body: &'static str,
}

const BUILTINS: [Builtin; 6] = [
    Builtin {
        name: "rewrite",
        description: "Rewrite the selection for clarity",
        hint: "",
        body: "Rewrite the selected text for clarity and flow. Keep its meaning, voice and every piece of markup. $ARGUMENTS",
    },
    Builtin {
        name: "shorten",
        description: "Make the selection shorter",
        hint: "",
        body: "Shorten the selected text. Keep its key claims, every citation and all markup. $ARGUMENTS",
    },
    Builtin {
        name: "translate",
        description: "Translate the selection",
        hint: "<language>",
        body: "Translate the selected text into $ARGUMENTS. Keep markup, math, citations and labels unchanged.",
    },
    Builtin {
        name: "explain",
        description: "Explain the selection",
        hint: "",
        body: "Explain the selected text briefly. Do not propose edits. $ARGUMENTS",
    },
    Builtin {
        name: "fix",
        description: "Fix the problem under the cursor",
        hint: "",
        body: "Fix the reported problem with the smallest change that removes it. Then check the result with diagnostics (with_pending). $ARGUMENTS",
    },
    Builtin {
        name: "cite",
        description: "Find citations for the selection",
        hint: "",
        body: "Find support in the project's bibliography for the claim in the selected text, using bib_search. Cite only keys it returns. If nothing matches, say so and change nothing. $ARGUMENTS",
    },
];

/// The built-in commands.
pub fn builtins() -> Vec<SlashCommand> {
    BUILTINS
        .iter()
        .map(|b| SlashCommand {
            name: b.name.to_owned(),
            description: b.description.to_owned(),
            hint: b.hint.to_owned(),
            body: b.body.to_owned(),
            source: Source::Builtin,
        })
        .collect()
}

/// A command name: lowercase letters, digits, `-` and `_`, at most 32 characters.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Parses a command file: optional front matter with `description:`, then the body.
fn parse_file(name: &str, text: &str, source: Source) -> SlashCommand {
    let mut description = String::new();
    let mut body = text;
    if let Some(rest) = text.strip_prefix("---\n")
        && let Some(end) = rest.find("\n---")
    {
        for line in rest[..end].lines() {
            if let Some(v) = line.strip_prefix("description:") {
                description = v.trim().trim_matches('"').to_owned();
            }
        }
        body = rest[end + 4..].trim_start_matches('\n');
    }
    let body: String = body.trim().chars().take(MAX_BODY).collect();
    SlashCommand {
        name: name.to_owned(),
        description: if description.is_empty() {
            "Custom command".to_owned()
        } else {
            description
        },
        hint: String::new(),
        body,
        source,
    }
}

fn read_dir(dir: &Path, source: Source, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries.filter_map(Result::ok).collect();
    files.sort_by_key(std::fs::DirEntry::file_name);
    for entry in files.into_iter().take(MAX_FILES) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !valid_name(name) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let cmd = parse_file(name, &text, source);
        if !cmd.body.is_empty() {
            out.push(cmd);
        }
    }
}

/// Every command, sorted by name: built-ins, then user files, then project files, each layer
/// replacing a same-named command of the layers before it.
pub fn load(project_root: Option<&Path>, config_dir: &Path) -> Vec<SlashCommand> {
    let mut layers = builtins();
    read_dir(&config_dir.join("commands"), Source::User, &mut layers);
    if let Some(root) = project_root {
        read_dir(
            &root.join(".manang").join("commands"),
            Source::Project,
            &mut layers,
        );
    }
    let mut merged: Vec<SlashCommand> = Vec::new();
    for cmd in layers {
        match merged.iter_mut().find(|c| c.name == cmd.name) {
            Some(slot) => *slot = cmd,
            None => merged.push(cmd),
        }
    }
    merged.sort_by(|a, b| a.name.cmp(&b.name));
    merged
}

/// The message a command stands for. `$ARGUMENTS` is replaced; a body without it gets the
/// arguments appended on a new line. `translate` defaults to English.
pub fn expand(command: &SlashCommand, arguments: &str) -> String {
    let args = arguments.trim();
    let args = if args.is_empty() && command.name == "translate" {
        "English"
    } else {
        args
    };
    let text = if command.body.contains("$ARGUMENTS") {
        command.body.replace("$ARGUMENTS", args)
    } else if args.is_empty() {
        command.body.clone()
    } else {
        format!("{}\n\n{args}", command.body)
    };
    text.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_expand_with_their_arguments() {
        let all = builtins();
        let get = |n: &str| all.iter().find(|c| c.name == n).expect("command");
        assert_eq!(
            expand(get("translate"), "German"),
            "Translate the selected text into German. Keep markup, math, citations and labels unchanged."
        );
        assert!(expand(get("translate"), "").contains("into English."));
        assert!(expand(get("rewrite"), "").ends_with("every piece of markup."));
        assert!(expand(get("rewrite"), "more formal").ends_with("more formal"));
        assert!(expand(get("fix"), "").contains("diagnostics (with_pending)"));
    }

    #[test]
    fn project_files_override_user_files_override_built_ins() {
        let config = tempfile::tempdir().expect("dir");
        let project = tempfile::tempdir().expect("dir");
        std::fs::create_dir_all(config.path().join("commands")).expect("mkdir");
        std::fs::create_dir_all(project.path().join(".manang/commands")).expect("mkdir");
        std::fs::write(
            config.path().join("commands/rewrite.md"),
            "---\ndescription: User rewrite\n---\nUser body.",
        )
        .expect("write");
        std::fs::write(
            project.path().join(".manang/commands/rewrite.md"),
            "Project body for $ARGUMENTS.",
        )
        .expect("write");
        std::fs::write(
            project.path().join(".manang/commands/Bad Name.md"),
            "ignored",
        )
        .expect("write");
        std::fs::write(
            project.path().join(".manang/commands/abstract.md"),
            "Write an abstract.",
        )
        .expect("write");
        let all = load(Some(project.path()), config.path());
        let rewrite = all.iter().find(|c| c.name == "rewrite").expect("rewrite");
        assert_eq!(rewrite.source, Source::Project);
        assert_eq!(expand(rewrite, "now"), "Project body for now.");
        assert!(
            all.iter()
                .any(|c| c.name == "abstract" && c.source == Source::Project)
        );
        assert!(!all.iter().any(|c| c.name.contains(' ')));
        let user_only = load(None, config.path());
        let rewrite = user_only
            .iter()
            .find(|c| c.name == "rewrite")
            .expect("rewrite");
        assert_eq!(rewrite.description, "User rewrite");
        assert_eq!(rewrite.source, Source::User);
        assert_eq!(all.len(), 7);
    }

    #[test]
    fn names_are_checked() {
        assert!(valid_name("fix"));
        assert!(valid_name("my-cmd_2"));
        assert!(!valid_name(""));
        assert!(!valid_name("../x"));
        assert!(!valid_name("Fix"));
    }
}
