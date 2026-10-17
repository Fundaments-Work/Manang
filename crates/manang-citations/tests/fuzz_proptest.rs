use manang_citations::{extract_keys, parse_bibtex, parse_csl_json};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn arbitrary_bibtex_never_panics(input in ".*") {
        // Must never panic on arbitrary input, returning Ok or Err
        let _ = parse_bibtex(&input, None);
    }

    #[test]
    fn arbitrary_csl_json_never_panics(input in ".*") {
        // Must never panic on arbitrary input, returning Ok or Err
        let _ = parse_csl_json(&input, None);
    }

    #[test]
    fn arbitrary_extract_keys_never_panics(input in ".*") {
        for syntax in ["typst", "latex", "markdown"] {
            let _ = extract_keys(&input, syntax);
        }
    }
}
