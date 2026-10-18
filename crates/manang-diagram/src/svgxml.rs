//! Attribute-level SVG rewriting with a real XML tokenizer.
//!
//! Some design rules cannot be expressed through an engine's options:
//! Mermaid draws rounded rectangles for some shapes, Pikchr has no theme at
//! all. These passes rewrite attributes event by event with `quick-xml`, so
//! markup inside text or CDATA is never touched. CSS variables are never
//! introduced: usvg and Typst do not resolve them.

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};

/// What to change in an SVG document.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rewrite<'a> {
    /// Paint a backdrop `<rect>` of this colour behind the drawing, sized to
    /// the root `viewBox` (a CSS background is ignored by usvg and Typst).
    pub background: Option<&'a str>,
    /// Force `rx`/`ry` on rectangles to zero and drop `filter` references,
    /// so there are no rounded corners or shadows.
    pub square: bool,
    /// Colour replacements applied to `fill`, `stroke` and `color`
    /// attributes and the same properties inside `style` attributes.
    /// Matching is exact on the trimmed value.
    pub recolor: &'a [(&'a str, &'a str)],
    /// Font family to set on `<text>` elements that have none.
    pub text_font: Option<&'a str>,
}

/// Square corners, no shadows, optional backdrop: the rules every engine's
/// output gets.
pub fn apply_design(svg: &str, background: Option<&str>) -> Result<String, String> {
    rewrite(
        svg,
        &Rewrite {
            background,
            square: true,
            ..Rewrite::default()
        },
    )
}

const PAINT_PROPERTIES: [&str; 3] = ["fill", "stroke", "color"];

fn recolor_value<'v>(value: &'v str, map: &[(&str, &'v str)]) -> Option<&'v str> {
    let value = value.trim();
    map.iter()
        .find(|(from, _)| from.eq_ignore_ascii_case(value))
        .map(|(_, to)| *to)
}

/// Rewrites `fill:..;stroke:..` declarations inside a `style` attribute.
fn recolor_style(style: &str, map: &[(&str, &str)]) -> Option<String> {
    let mut changed = false;
    let parts: Vec<String> = style
        .split(';')
        .map(|decl| {
            if let Some((prop, value)) = decl.split_once(':')
                && PAINT_PROPERTIES.contains(&prop.trim())
                && let Some(to) = recolor_value(value, map)
            {
                changed = true;
                format!("{}:{to}", prop.trim())
            } else {
                decl.to_owned()
            }
        })
        .collect();
    changed.then(|| parts.join(";"))
}

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
        if !plan.recolor.is_empty() {
            let k = key;
            if PAINT_PROPERTIES.contains(&k)
                && let Some(to) = recolor_value(&attr.value, plan.recolor)
            {
                out.push_attribute((k, to));
                continue;
            }
            if k == "style"
                && let Some(style) = recolor_style(&attr.value, plan.recolor)
            {
                out.push_attribute(("style", style.as_str()));
                continue;
            }
        }
        out.push_attribute(Attribute {
            key: attr.key,
            value: attr.value,
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

/// Applies `plan` to `svg`, returning the rewritten document.
pub fn rewrite(svg: &str, plan: &Rewrite<'_>) -> Result<String, String> {
    let mut reader = Reader::from_str(svg);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(svg.len() + 128));
    let mut seen_root = false;
    // Depth inside a dropped `<filter>` element (shadow definitions).
    let mut skipping = 0usize;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
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
                let element = rewrite_element(&start, plan)?;
                let backdrop = if root {
                    plan.background.zip(view_box(&start))
                } else {
                    None
                };
                writer
                    .write_event(Event::Start(element))
                    .map_err(|e| e.to_string())?;
                if let Some((bg, [x, y, w, h])) = backdrop {
                    let mut rect = BytesStart::new("rect");
                    rect.push_attribute(("x", x.as_str()));
                    rect.push_attribute(("y", y.as_str()));
                    rect.push_attribute(("width", w.as_str()));
                    rect.push_attribute(("height", h.as_str()));
                    rect.push_attribute(("fill", bg));
                    writer
                        .write_event(Event::Empty(rect))
                        .map_err(|e| e.to_string())?;
                }
            }
            Event::Empty(start) => {
                seen_root = true;
                let element = rewrite_element(&start, plan)?;
                writer
                    .write_event(Event::Empty(element))
                    .map_err(|e| e.to_string())?;
            }
            other => writer.write_event(other).map_err(|e| e.to_string())?,
        }
    }
    String::from_utf8(writer.into_inner()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squares_rects_and_drops_filters_but_not_text() {
        let svg = r#"<svg viewBox="0 0 10 20"><defs><filter id="s"><feDropShadow dx="4"/></filter></defs><rect rx="5" ry="5" filter="url(#s)"/><text>rx="5" &lt;rect&gt;</text></svg>"#;
        let out = apply_design(svg, Some("#ffffff")).unwrap_or_default();
        assert_eq!(
            out,
            r##"<svg viewBox="0 0 10 20"><rect x="0" y="0" width="10" height="20" fill="#ffffff"/><defs></defs><rect rx="0" ry="0"/><text>rx="5" &lt;rect&gt;</text></svg>"##
        );
    }

    #[test]
    fn recolours_attributes_and_styles() {
        let svg = r#"<svg><path style="fill:rgb(0,0,0);stroke:rgb(1,2,3)"/><text fill="rgb(0,0,0)">a</text></svg>"#;
        let out = rewrite(
            svg,
            &Rewrite {
                recolor: &[("rgb(0,0,0)", "#e8e6e1")],
                text_font: Some("DM Sans, sans-serif"),
                ..Rewrite::default()
            },
        )
        .unwrap_or_default();
        assert_eq!(
            out,
            r##"<svg><path style="fill:#e8e6e1;stroke:rgb(1,2,3)"/><text fill="#e8e6e1" font-family="DM Sans, sans-serif">a</text></svg>"##
        );
    }
}
