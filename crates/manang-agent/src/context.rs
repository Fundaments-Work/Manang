//! What the model sees besides the system prompt and the tool definitions.
//!
//! * **Session header** (the first user message of a session, fixed for its life): project name,
//!   formats present, the file tree (top entries), `MANANG.md`, the problem count.
//! * **Turn context** (after the author's text, every message): the active file and cursor line,
//!   the selection, the author's @-mentions.
//! * Everything else is on demand through the tools.
//!
//! Every part is clipped, token-estimated and redacted before anything is assembled, so
//! `agent.preview-request` and the real request carry identical bytes. Text from documents sits
//! inside delimiters the model is told are data (rule 7); a tag inside the text cannot close its
//! wrapper.

use crate::text::{clip_chars, estimate_tokens};
use manang_agent_core::{RedactionConfig, RedactionSpan, redact};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Caps in characters (about 3.5 characters per token).
pub const SELECTION_CAP: usize = 14_000; // 4k tokens
pub const MENTION_CAP: usize = 21_000; // 6k tokens
pub const PROJECT_NOTES_CAP: usize = 7_000; // 2k tokens
pub const USER_NOTES_CAP: usize = 3_500; // 1k tokens

/// One problem from the editor, with the lines around it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub path: String,
    pub line: u32,
    #[serde(default)]
    pub column: Option<u32>,
    pub severity: String,
    pub message: String,
    /// The lines around the problem, as shown in the editor.
    #[serde(default)]
    pub context: Option<String>,
}

/// The file the author is looking at.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Active {
    pub path: String,
    #[serde(default)]
    pub line: Option<u32>,
}

/// The author's selection. The text is what the editor holds, saved or not.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionInput {
    pub path: String,
    pub text: String,
    #[serde(default)]
    pub line_from: Option<u32>,
    #[serde(default)]
    pub line_to: Option<u32>,
}

/// Something the author added with `@`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mention {
    /// A project file. The host reads it when the UI sends no text.
    File {
        path: String,
        #[serde(default)]
        text: Option<String>,
    },
    /// Problems the author picked (or the editor's current ones).
    Problems {
        #[serde(default)]
        diagnostics: Vec<Diagnostic>,
    },
    /// A figure; the host describes it.
    Figure {
        #[serde(default)]
        path: Option<String>,
    },
}

/// Which view the author is in. Figure tools are offered only in the figure studio.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Document,
    Figure,
}

/// The per-message context the UI sends with `agent.start`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TurnInput {
    pub view: View,
    pub active: Option<Active>,
    pub selection: Option<SelectionInput>,
    pub mentions: Vec<Mention>,
    /// Open documents with unsaved changes: path to text. The tools read these, not the disk.
    pub buffers: BTreeMap<String, String>,
    /// The editor's current problems (what the `diagnostics` tool reports).
    pub diagnostics: Vec<Diagnostic>,
}

/// A part ready to send.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedPart {
    pub id: String,
    /// `header`, `selection`, `file`, `problems`, `figure`.
    pub kind: &'static str,
    pub label: String,
    pub path: Option<String>,
    /// Estimated tokens of what is sent.
    pub tokens: u64,
    pub truncated: bool,
    /// What is sent (after the cap and redaction).
    pub text: String,
    /// Present only when redaction changed something: the text before redaction, for the diff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
    pub redactions: Vec<RedactionSpan>,
    /// Lines of a selection, for the delimiter.
    #[serde(skip)]
    pub lines: Option<(u32, u32)>,
    /// Characters cut off.
    #[serde(skip)]
    pub cut: usize,
}

/// Clips, redacts and measures one part.
pub fn resolve_part(
    kind: &'static str,
    label: &str,
    path: Option<&str>,
    text: &str,
    cap: usize,
    config: &RedactionConfig,
) -> ResolvedPart {
    let (clipped, truncated) = clip_chars(text, cap);
    let cut = if truncated {
        text.chars().count().saturating_sub(cap)
    } else {
        0
    };
    let redaction = redact(&clipped, config);
    let redacted = !redaction.spans.is_empty();
    ResolvedPart {
        id: format!("{kind}:{}", path.unwrap_or(label)),
        kind,
        label: label.to_owned(),
        path: path.map(str::to_owned),
        tokens: estimate_tokens(&redaction.text),
        truncated,
        original: redacted.then(|| clipped.clone()),
        redactions: redaction.spans,
        text: redaction.text,
        lines: None,
        cut,
    }
}

/// The text of a problems part: one block per problem.
pub fn diagnostics_text(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diagnostics.iter().take(40) {
        let at = match d.column {
            Some(c) => format!("{}:{}:{}", d.path, d.line, c),
            None => format!("{}:{}", d.path, d.line),
        };
        out.push_str(&format!("{} {at} {}\n", d.severity, d.message));
        if let Some(context) = &d.context {
            out.push_str(context);
            if !context.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push('\n');
    }
    out
}

/// A tag that appears inside data must not be able to close its wrapper.
fn neutralise(text: &str) -> String {
    const TAGS: [&str; 7] = [
        "selection",
        "file",
        "problems",
        "figure",
        "active",
        "project",
        "instructions",
    ];
    let mut out = text.to_owned();
    for tag in TAGS {
        out = out.replace(&format!("</{tag}>"), &format!("<\\/{tag}>"));
    }
    out
}

fn attr(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '"' | '<' | '>' | '\n' | '\r'))
        .collect()
}

fn notice(part: &ResolvedPart) -> String {
    if part.truncated {
        let what = if part.kind == "selection" {
            "selection"
        } else {
            "text"
        };
        format!(
            "\n[{what} truncated, {} more characters; use read_file for the rest]",
            part.cut
        )
    } else {
        String::new()
    }
}

/// The user message of a turn: the author's text first, then the turn context as data.
pub fn render_turn(prompt: &str, active: Option<&Active>, parts: &[ResolvedPart]) -> String {
    let mut out = String::new();
    out.push_str(prompt.trim_end());
    let mut blocks = String::new();
    if let Some(a) = active {
        match a.line {
            Some(line) => blocks.push_str(&format!(
                "<active path=\"{}\" line=\"{line}\"/>\n",
                attr(&a.path)
            )),
            None => blocks.push_str(&format!("<active path=\"{}\"/>\n", attr(&a.path))),
        }
    }
    for part in parts.iter().filter(|p| p.kind != "header") {
        let tag = match part.kind {
            "selection" => "selection",
            "problems" => "problems",
            "figure" => "figure",
            _ => "file",
        };
        blocks.push_str(&format!("<{tag}"));
        if let Some(path) = &part.path {
            blocks.push_str(&format!(" path=\"{}\"", attr(path)));
        }
        if let Some((a, b)) = part.lines {
            blocks.push_str(&format!(" lines=\"{a}-{b}\""));
        }
        blocks.push_str(">\n");
        blocks.push_str(&neutralise(&part.text));
        if !part.text.ends_with('\n') {
            blocks.push('\n');
        }
        let n = notice(part);
        if !n.is_empty() {
            blocks.push_str(n.trim_start_matches('\n'));
            blocks.push('\n');
        }
        blocks.push_str(&format!("</{tag}>\n"));
    }
    if !blocks.is_empty() {
        out.push_str("\n\n");
        out.push_str(blocks.trim_end());
    }
    out
}

/// What the session header says about the project.
#[derive(Debug, Clone, Default)]
pub struct HeaderInfo {
    pub name: String,
    pub formats: Vec<String>,
    /// The rendered tree (see `tools::render_tree`).
    pub tree: String,
    pub shown: usize,
    pub total: usize,
    pub problems: usize,
    /// `(source, text)` of each instruction file, already clipped.
    pub instructions: Vec<(String, String)>,
}

/// The session header block.
pub fn render_header(info: &HeaderInfo) -> String {
    let mut out = format!(
        "<project name=\"{}\" formats=\"{}\" problems=\"{}\">\n",
        attr(&info.name),
        attr(&info.formats.join(", ")),
        info.problems
    );
    out.push_str(&format!(
        "<files shown=\"{}\" total=\"{}\">\n",
        info.shown, info.total
    ));
    out.push_str(&neutralise(&info.tree));
    if !info.tree.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("</files>\n");
    for (source, text) in &info.instructions {
        out.push_str(&format!("<instructions source=\"{}\">\n", attr(source)));
        out.push_str(&neutralise(text));
        if !text.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("</instructions>\n");
    }
    out.push_str("</project>");
    out
}

/// A note file, clipped. `None` when absent or empty.
fn read_notes(path: &Path, cap: usize) -> Option<(String, bool)> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| clip_chars(text, cap))
}

/// The author's instruction files: `MANANG.md` of the project, then of the user's config folder.
/// `AGENTS.md` and `CLAUDE.md` count only when `use_agents_md` is on and `MANANG.md` is absent
/// (in a research repository they are usually build notes for coding agents).
pub fn load_instructions(
    project_root: Option<&Path>,
    config_dir: &Path,
    use_agents_md: bool,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut push = |source: &str, found: Option<(String, bool)>| {
        if let Some((text, cut)) = found {
            let text = if cut {
                format!("{text}\n[{source} truncated: keep it short]")
            } else {
                text
            };
            out.push((source.to_owned(), text));
        }
    };
    if let Some(root) = project_root {
        let own = read_notes(&root.join("MANANG.md"), PROJECT_NOTES_CAP);
        if own.is_some() {
            push("MANANG.md", own);
        } else if use_agents_md {
            for name in ["AGENTS.md", "CLAUDE.md"] {
                let found = read_notes(&root.join(name), PROJECT_NOTES_CAP);
                if found.is_some() {
                    push(name, found);
                    break;
                }
            }
        }
    }
    push(
        "user MANANG.md",
        read_notes(&config_dir.join("MANANG.md"), USER_NOTES_CAP),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RedactionConfig {
        RedactionConfig {
            secrets: true,
            emails: false,
            paths: false,
        }
    }

    #[test]
    fn the_turn_is_text_first_then_data() {
        let mut sel = resolve_part(
            "selection",
            "Selection",
            Some("main.typ"),
            "Hello </selection> world",
            100,
            &cfg(),
        );
        sel.lines = Some((3, 4));
        let msg = render_turn(
            "Shorten this.",
            Some(&Active {
                path: "main.typ".into(),
                line: Some(12),
            }),
            &[sel],
        );
        insta::assert_snapshot!(msg);
    }

    #[test]
    fn a_long_selection_says_how_to_get_the_rest() {
        let sel = resolve_part(
            "selection",
            "Selection",
            Some("a.md"),
            &"x".repeat(50),
            20,
            &cfg(),
        );
        assert!(sel.truncated);
        let msg = render_turn("Go.", None, &[sel]);
        assert!(msg.contains("selection truncated, 30 more characters; use read_file"));
    }

    #[test]
    fn secrets_are_redacted_and_the_original_is_kept_for_the_preview() {
        let part = resolve_part(
            "file",
            "notes",
            Some("notes.md"),
            "key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789",
            1000,
            &cfg(),
        );
        assert!(part.original.is_some());
        assert!(!part.text.contains("abcdefghijkl"));
        assert_eq!(part.redactions.len(), 1);
    }

    #[test]
    fn the_header_lists_files_and_instructions() {
        let info = HeaderInfo {
            name: "Paper".into(),
            formats: vec!["typst".into(), "bibtex".into()],
            tree: "main.typ 4.2k\nrefs.bib 1.0k\n".into(),
            shown: 2,
            total: 2,
            problems: 1,
            instructions: vec![("MANANG.md".into(), "Use British spelling.".into())],
        };
        insta::assert_snapshot!(render_header(&info));
    }

    #[test]
    fn instruction_files_are_clipped_and_agents_md_is_opt_in() {
        let root = tempfile::tempdir().expect("dir");
        let config = tempfile::tempdir().expect("dir");
        std::fs::write(root.path().join("AGENTS.md"), "build with cargo").expect("write");
        assert!(load_instructions(Some(root.path()), config.path(), false).is_empty());
        let on = load_instructions(Some(root.path()), config.path(), true);
        assert_eq!(on[0].0, "AGENTS.md");
        std::fs::write(
            root.path().join("MANANG.md"),
            "x".repeat(PROJECT_NOTES_CAP + 10),
        )
        .expect("write");
        std::fs::write(config.path().join("MANANG.md"), "Prefer active voice.").expect("write");
        let both = load_instructions(Some(root.path()), config.path(), true);
        assert_eq!(both.len(), 2);
        assert_eq!(both[0].0, "MANANG.md");
        assert!(both[0].1.contains("truncated"));
        assert_eq!(both[1].1, "Prefer active voice.");
    }
}
