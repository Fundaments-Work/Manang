//! Kernel plugin interface for the markdown engine.

use manang_kernel::{Context, Manifest, Payload, Plugin, PluginError};
use serde_json::{Value, json};

use crate::diagrams::{DiagramFence, diagram_fences};
use crate::frontmatter::parse_frontmatter;
use crate::html::{RenderOptions, render_to_html_at};
use crate::outline::extract_outline;
use crate::typst::{FenceFigure, markdown_to_typst_with};

const MANIFEST: &str = include_str!("../manifest.toml");

/// The markdown plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Builds the markdown plugin instance.
pub fn plugin() -> impl Plugin {
    MarkdownPlugin::new()
}

/// Exposes markdown parsing, outline extraction, HTML rendering, and Typst conversion as kernel commands.
#[derive(Default, Clone)]
pub struct MarkdownPlugin;

impl MarkdownPlugin {
    /// Creates a new markdown plugin instance.
    pub fn new() -> Self {
        Self
    }
}

fn args(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(PluginError::Failed("expected a JSON payload".to_owned())),
    }
}

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, PluginError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Failed(format!("`{key}` must be a string")))
}

fn failed(error: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(error.to_string())
}

/// Lines before `body` in `source` (the frontmatter), when `body` is the
/// tail of `source` as [`parse_frontmatter`] returns it.
fn body_line_offset(source: &str, body: &str) -> usize {
    source
        .strip_suffix(body)
        .map_or(0, |head| head.matches('\n').count())
}

/// The diagram plugin's `[code] line L:C: message` as `line N: message`,
/// with `L` moved from the fence's first line to the document's.
fn document_error(fence: &DiagramFence, error: &str) -> String {
    let message = error
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("] "))
        .map_or(error, |(_, message)| message);
    let parsed = message.strip_prefix("line ").and_then(|rest| {
        let (position, text) = rest.split_once(": ")?;
        let line = position.split(':').next()?.parse::<usize>().ok()?;
        Some(format!("line {}: {text}", fence.line + line))
    });
    parsed.unwrap_or_else(|| message.to_owned())
}

/// A diagram fence's figure: rendered (print theme) into the project's
/// diagram cache by `project.cache-diagram`, or kept as code with a warning
/// that says why.
fn cached_figure(ctx: &Context<'_>, fence: &DiagramFence) -> FenceFigure {
    const COMMAND: &str = "project.cache-diagram";
    let kept = |why: String| FenceFigure::Keep(format!("{why}; it stays a code block"));
    if !ctx.has_command(COMMAND) {
        return kept(format!(
            "no project plugin can render the {} diagram",
            fence.language
        ));
    }
    let args = json!({ "language": fence.language, "source": fence.source });
    match ctx.invoke(COMMAND, Payload::Json(args)) {
        Ok(Payload::Json(out)) => match (out.get("ok"), out.get("svg").and_then(Value::as_str)) {
            (Some(Value::Bool(true)), Some(svg)) => FenceFigure::Image(format!("/{svg}")),
            _ => {
                let error = out
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("an unknown error");
                kept(format!(
                    "the {} diagram has an error at {}",
                    fence.language,
                    document_error(fence, error)
                ))
            }
        },
        Ok(_) => kept("the project plugin returned no file".to_owned()),
        Err(e) => kept(format!(
            "the {} diagram was not rendered ({e})",
            fence.language
        )),
    }
}

impl Plugin for MarkdownPlugin {
    fn invoke(
        &self,
        ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let a = args(input)?;
        match command {
            "markdown.render-html" => {
                let source = string(&a, "source")?;
                let (frontmatter, body, mut fm_diags) = parse_frontmatter(source);

                let render_opts: RenderOptions = match a.get("options") {
                    Some(v) => serde_json::from_value(v.clone()).map_err(failed)?,
                    None => {
                        let allow_raw_html = a
                            .get("allow_raw_html")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let allowed_html_tags = match a.get("allowed_html_tags") {
                            Some(Value::Array(arr)) => arr
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect(),
                            _ => vec![
                                "b".to_string(),
                                "i".to_string(),
                                "em".to_string(),
                                "strong".to_string(),
                                "span".to_string(),
                                "sub".to_string(),
                                "sup".to_string(),
                                "kbd".to_string(),
                                "br".to_string(),
                                "p".to_string(),
                                "code".to_string(),
                            ],
                        };
                        RenderOptions {
                            allow_raw_html,
                            allowed_html_tags,
                        }
                    }
                };

                let (html, mut html_diags) =
                    render_to_html_at(body, &render_opts, body_line_offset(source, body));
                let headings = extract_outline(body);

                fm_diags.append(&mut html_diags);

                let res = json!({
                    "html": html,
                    "frontmatter": frontmatter,
                    "headings": headings,
                    "diagnostics": fm_diags,
                    "diagrams": diagram_fences(source),
                });
                Ok(Payload::Json(res))
            }
            "markdown.outline" => {
                let source = string(&a, "source")?;
                let (_, body, _) = parse_frontmatter(source);
                let headings = extract_outline(body);
                Ok(Payload::Json(json!({ "headings": headings })))
            }
            "markdown.diagrams" => {
                let source = string(&a, "source")?;
                Ok(Payload::Json(json!({ "diagrams": diagram_fences(source) })))
            }
            "markdown.to-typst" => {
                let source = string(&a, "source")?;
                let (_, body, mut fm_diags) = parse_frontmatter(source);
                let (typst, mut typst_diags) =
                    markdown_to_typst_with(body, body_line_offset(source, body), &mut |fence| {
                        Some(cached_figure(ctx, fence))
                    });
                fm_diags.append(&mut typst_diags);
                Ok(Payload::Json(json!({
                    "typst": typst,
                    "diagnostics": fm_diags,
                })))
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
