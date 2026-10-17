//! Property-based fuzz tests verifying that arbitrary input never panics.

use manang_markdown::{
    RenderOptions, extract_outline, markdown_to_typst, parse_frontmatter, render_to_html,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Arbitrary Unicode strings must never panic when parsed for frontmatter,
    /// rendered to HTML, extracted for outlines, or converted to Typst.
    #[test]
    fn arbitrary_input_never_panics(input in "\\PC{0,500}") {
        let (fm, body, _fm_diags) = parse_frontmatter(&input);
        if let Some(fm) = fm {
            prop_assert!(!fm.format.is_empty());
        }

        let default_opts = RenderOptions::default();
        let (html, _html_diags) = render_to_html(body, &default_opts);
        prop_assert!(!html.is_empty() || body.is_empty());

        let permissive_opts = RenderOptions {
            allow_raw_html: true,
            allowed_html_tags: vec!["b".to_string(), "i".to_string()],
        };
        let (html_permissive, _) = render_to_html(body, &permissive_opts);
        let _ = html_permissive;

        let headings = extract_outline(body);
        for h in headings {
            prop_assert!(h.level >= 1 && h.level <= 6);
        }

        let (typst, _typst_diags) = markdown_to_typst(body);
        let _ = typst;
    }
}
