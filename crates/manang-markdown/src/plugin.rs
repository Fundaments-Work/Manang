//! Kernel plugin interface for the markdown engine.

use manang_kernel::{Context, Manifest, Payload, Plugin, PluginError};
use serde_json::{Value, json};

use crate::frontmatter::parse_frontmatter;
use crate::html::{RenderOptions, render_to_html};
use crate::outline::extract_outline;
use crate::typst::markdown_to_typst;

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

impl Plugin for MarkdownPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
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

                let (html, mut html_diags) = render_to_html(body, &render_opts);
                let headings = extract_outline(body);

                fm_diags.append(&mut html_diags);

                let res = json!({
                    "html": html,
                    "frontmatter": frontmatter,
                    "headings": headings,
                    "diagnostics": fm_diags,
                });
                Ok(Payload::Json(res))
            }
            "markdown.outline" => {
                let source = string(&a, "source")?;
                let (_, body, _) = parse_frontmatter(source);
                let headings = extract_outline(body);
                Ok(Payload::Json(json!({ "headings": headings })))
            }
            "markdown.to-typst" => {
                let source = string(&a, "source")?;
                let (_, body, mut fm_diags) = parse_frontmatter(source);
                let (typst, mut typst_diags) = markdown_to_typst(body);
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
