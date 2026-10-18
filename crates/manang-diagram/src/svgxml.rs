//! Attribute-level SVG rewriting with a real XML tokenizer.
//!
//! Some design rules cannot be expressed through an engine's options:
//! Mermaid draws rounded rectangles for some shapes and ships colours of its
//! own, Pikchr has no theme at all. These passes rewrite the document event
//! by event with `quick-xml`: attribute values and `<style>` contents are
//! edited, while text content (labels) is never touched. CSS variables are
//! removed, never introduced: usvg and Typst do not resolve them.

use std::borrow::Cow;

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesCData, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};

use crate::color::{self, Invert};
use crate::theme::Palette;

/// What to change in an SVG document.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rewrite<'a> {
    /// Paint a backdrop `<rect>` of this colour behind the drawing, sized to
    /// the root `viewBox` (a CSS background is ignored by usvg and Typst).
    pub background: Option<&'a str>,
    /// Force `rx`/`ry` on rectangles to zero and drop `filter` elements and
    /// references, so there are no rounded corners or shadows.
    pub square: bool,
    /// Map every colour that is not a token of this palette onto its tonal
    /// ramp (see [`crate::color`]), in paint attributes and `<style>`.
    pub palette: Option<(&'a Palette, Invert)>,
    /// Font family to set on `<text>` elements that have none.
    pub text_font: Option<&'a str>,
}

/// Attributes that carry paint.
const PAINT_ATTRIBUTES: [&str; 7] = [
    "fill",
    "stroke",
    "color",
    "stop-color",
    "flood-color",
    "lighting-color",
    "style",
];

fn view_box(start: &BytesStart<'_>) -> Option<[String; 4]> {
    let attr = start
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == "viewBox")?;
    let mut nums = attr
        .value
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    Some([nums.next()?, nums.next()?, nums.next()?, nums.next()?])
}

fn rewrite_element(
    start: &BytesStart<'_>,
    plan: &Rewrite<'_>,
) -> Result<BytesStart<'static>, String> {
    let name = start.name().as_ref().to_owned();
    let is_rect = name == "rect";
    let is_text = name == "text";
    let mut out = BytesStart::new(name);
    let mut has_font = false;
    for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let key = attr.key.as_ref();
        if plan.square && key == "filter" {
            continue;
        }
        if plan.square && is_rect && (key == "rx" || key == "ry") {
            out.push_attribute((key, "0"));
            continue;
        }
        if key == "font-family" {
            has_font = true;
        }
        let value = match plan.palette {
            Some((palette, invert)) if PAINT_ATTRIBUTES.contains(&key) => {
                // Values are raw (escaped) XML; colour tokens never contain
                // entities, so editing the raw text is safe.
                match color::clamp(&attr.value, palette, invert) {
                    Cow::Borrowed(_) => attr.value,
                    Cow::Owned(changed) => Cow::Owned(changed),
                }
            }
            _ => attr.value,
        };
        out.push_attribute(Attribute {
            key: attr.key,
            value,
        });
    }
    if is_text
        && !has_font
        && let Some(font) = plan.text_font
    {
        out.push_attribute(("font-family", font));
    }
    Ok(out.into_owned())
}

/// Rewrites CSS inside `<style>`: colours onto the palette, `var(--…)`
/// declarations removed.
fn rewrite_css<'t>(css: &'t str, plan: &Rewrite<'_>) -> Cow<'t, str> {
    let css = color::strip_css_vars(css);
    match plan.palette {
        Some((palette, invert)) => match color::clamp(&css, palette, invert) {
            Cow::Borrowed(_) => css,
            Cow::Owned(changed) => Cow::Owned(changed),
        },
        None => css,
    }
}

fn io(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Applies `plan` to `svg`, returning the rewritten document.
pub fn rewrite(svg: &str, plan: &Rewrite<'_>) -> Result<String, String> {
    let mut reader = Reader::from_str(svg);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(svg.len() + 128));
    let mut seen_root = false;
    // Depth inside a dropped `<filter>` element (shadow definitions).
    let mut skipping = 0usize;
    // Depth inside `<style>`.
    let mut in_style = 0usize;
    loop {
        let event = reader.read_event().map_err(io)?;
        if skipping > 0 {
            match event {
                Event::Start(_) => skipping += 1,
                Event::End(_) => skipping -= 1,
                Event::Eof => break,
                _ => {}
            }
            continue;
        }
        match event {
            Event::Eof => break,
            Event::Start(start) if plan.square && start.name().as_ref() == "filter" => {
                skipping = 1;
            }
            Event::Empty(start) if plan.square && start.name().as_ref() == "filter" => {}
            Event::Start(start) => {
                let root = !seen_root;
                seen_root = true;
                if start.name().as_ref() == "style" {
                    in_style += 1;
                }
                let element = rewrite_element(&start, plan)?;
                let backdrop = if root {
                    plan.background.zip(view_box(&start))
                } else {
                    None
                };
                writer.write_event(Event::Start(element)).map_err(io)?;
                if let Some((bg, [x, y, w, h])) = backdrop {
                    let mut rect = BytesStart::new("rect");
                    rect.push_attribute(("x", x.as_str()));
                    rect.push_attribute(("y", y.as_str()));
                    rect.push_attribute(("width", w.as_str()));
                    rect.push_attribute(("height", h.as_str()));
                    rect.push_attribute(("fill", bg));
                    writer.write_event(Event::Empty(rect)).map_err(io)?;
                }
            }
            Event::End(end) => {
                if end.name().as_ref() == "style" {
                    in_style = in_style.saturating_sub(1);
                }
                writer.write_event(Event::End(end)).map_err(io)?;
            }
            Event::Empty(start) => {
                seen_root = true;
                let element = rewrite_element(&start, plan)?;
                writer.write_event(Event::Empty(element)).map_err(io)?;
            }
            Event::Text(text) if in_style > 0 => {
                let css = rewrite_css(&text, plan).into_owned();
                writer
                    .write_event(Event::Text(BytesText::from_escaped(css)))
                    .map_err(io)?;
            }
            Event::CData(data) if in_style > 0 => {
                let css = rewrite_css(&data, plan).into_owned();
                writer
                    .write_event(Event::CData(BytesCData::new(css)))
                    .map_err(io)?;
            }
            other => writer.write_event(other).map_err(io)?,
        }
    }
    String::from_utf8(writer.into_inner()).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::DARK;

    #[test]
    fn squares_rects_and_drops_filters_but_not_text() {
        let svg = r#"<svg viewBox="0 0 10 20"><defs><filter id="s"><feDropShadow dx="4"/></filter></defs><rect rx="5" ry="5" filter="url(#s)"/><text>rx="5" &lt;rect&gt;</text></svg>"#;
        let plan = Rewrite {
            background: Some("#ffffff"),
            square: true,
            ..Rewrite::default()
        };
        assert_eq!(
            rewrite(svg, &plan).unwrap_or_default(),
            r##"<svg viewBox="0 0 10 20"><rect x="0" y="0" width="10" height="20" fill="#ffffff"/><defs></defs><rect rx="0" ry="0"/><text>rx="5" &lt;rect&gt;</text></svg>"##
        );
    }

    #[test]
    fn recolours_attributes_styles_and_css_but_not_labels() {
        let svg = r#"<svg><style>.a{fill:black;font:var(--x)}</style><path style="fill:rgb(0,0,0);stroke:#1c1c20"/><text fill="black">black</text></svg>"#;
        let plan = Rewrite {
            palette: Some((&DARK, Invert::All)),
            text_font: Some("DM Sans, sans-serif"),
            ..Rewrite::default()
        };
        assert_eq!(
            rewrite(svg, &plan).unwrap_or_default(),
            r##"<svg><style>.a{fill:#e8e6e1;}</style><path style="fill:#e8e6e1;stroke:#1c1c20"/><text fill="#e8e6e1" font-family="DM Sans, sans-serif">black</text></svg>"##
        );
    }
}
