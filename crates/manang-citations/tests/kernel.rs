use manang_citations::{Author, Entry, MockHttpClient, manifest, plugin_with_client};
use manang_kernel::{Kernel, Payload, Tier};
use serde_json::json;
use std::sync::Arc;

fn test_kernel(mock_http: Arc<MockHttpClient>) -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest is valid"),
            Arc::new(plugin_with_client(mock_http)),
        )
        .expect("registers");
    kernel
}

#[test]
fn manifest_is_native_and_commands_registered() {
    let mock = Arc::new(MockHttpClient::new());
    let k = test_kernel(mock);

    let m = manifest().expect("manifest");
    assert_eq!(m.plugin.name, "citations");
    assert_eq!(m.plugin.tier, Tier::Native);

    let ids: Vec<_> = k.commands().into_iter().map(|c| c.id).collect();
    for id in [
        "citations.load",
        "citations.search",
        "citations.get",
        "citations.format",
        "citations.extract-keys",
        "citations.verify",
    ] {
        assert!(ids.contains(&id.to_owned()), "missing command {id}");
    }
}

#[test]
fn load_and_get_bibtex_and_csl() {
    let mock = Arc::new(MockHttpClient::new());
    let k = test_kernel(mock);

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let bib_path = temp_dir.path().join("references.bib");
    let csl_path = temp_dir.path().join("references.json");

    let bib_content = r#"
@article{einstein1905,
    author = {Albert Einstein},
    title = {Zur Elektrodynamik bewegter K{\"o}rper},
    journal = {Annalen der Physik},
    volume = {322},
    number = {10},
    pages = {891--921},
    year = {1905},
    doi = {10.1002/andp.19053221004}
}
"#;

    let csl_content = r#"[
  {
    "id": "shannon1948",
    "type": "article-journal",
    "title": "A Mathematical Theory of Communication",
    "author": [
      { "family": "Shannon", "given": "Claude E." }
    ],
    "issued": {
      "date-parts": [[1948, 7]]
    },
    "container-title": "Bell System Technical Journal",
    "volume": "27",
    "issue": "3",
    "page": "379-423",
    "DOI": "10.1002/j.1538-7305.1948.tb01338.x"
  }
]"#;

    std::fs::write(&bib_path, bib_content).expect("write bib");
    std::fs::write(&csl_path, csl_content).expect("write csl");

    // 1. Load files
    let load_res = k
        .invoke(
            "citations.load",
            Payload::Json(json!({
                "paths": [
                    bib_path.to_str().unwrap(),
                    csl_path.to_str().unwrap(),
                ]
            })),
        )
        .expect("citations.load invoke");

    let load_val = match load_res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };
    assert_eq!(load_val["loaded"], 2);
    assert_eq!(load_val["total"], 2);
    assert!(load_val["errors"].as_array().unwrap().is_empty());

    // 2. Get BibTeX entry
    let einstein_res = k
        .invoke(
            "citations.get",
            Payload::Json(json!({ "key": "einstein1905" })),
        )
        .expect("get einstein");
    let einstein_val = match einstein_res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };
    assert_eq!(einstein_val["key"], "einstein1905");
    assert_eq!(einstein_val["year"], 1905);
    assert_eq!(einstein_val["volume"], "322");
    assert_eq!(einstein_val["doi"], "10.1002/andp.19053221004");
    let authors = einstein_val["authors"].as_array().expect("authors array");
    assert_eq!(authors.len(), 1);
    assert_eq!(authors[0]["family"], "Einstein");

    // 3. Get CSL entry
    let shannon_res = k
        .invoke(
            "citations.get",
            Payload::Json(json!({ "key": "shannon1948" })),
        )
        .expect("get shannon");
    let shannon_val = match shannon_res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };
    assert_eq!(shannon_val["key"], "shannon1948");
    assert_eq!(
        shannon_val["title"],
        "A Mathematical Theory of Communication"
    );
    assert_eq!(shannon_val["year"], 1948);
    assert_eq!(shannon_val["month"], 7);
}

#[test]
fn search_index_and_ranking() {
    let mock = Arc::new(MockHttpClient::new());
    let k = test_kernel(mock);

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let bib_path = temp_dir.path().join("papers.bib");

    let bib = r#"
@article{einstein1905,
    author = {Albert Einstein},
    title = {On the Electrodynamics of Moving Bodies},
    year = {1905}
}
@article{shannon1948,
    author = {Claude E. Shannon},
    title = {A Mathematical Theory of Communication},
    year = {1948}
}
@article{turing1936,
    author = {Alan M. Turing},
    title = {On Computable Numbers with an Application to the Entscheidungsproblem},
    year = {1936}
}
"#;
    std::fs::write(&bib_path, bib).expect("write bib");

    k.invoke(
        "citations.load",
        Payload::Json(json!({ "path": bib_path.to_str().unwrap() })),
    )
    .expect("load");

    // Search by author
    let res1 = k
        .invoke(
            "citations.search",
            Payload::Json(json!({ "query": "Einstein" })),
        )
        .expect("search einstein");
    let val1 = match res1 {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let items1 = val1["results"].as_array().expect("results array");
    assert!(!items1.is_empty());
    assert_eq!(items1[0]["key"], "einstein1905");

    // Search by title keywords
    let res2 = k
        .invoke(
            "citations.search",
            Payload::Json(json!({ "query": "communication theory" })),
        )
        .expect("search communication");
    let val2 = match res2 {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let items2 = val2["results"].as_array().expect("results array");
    assert!(!items2.is_empty());
    assert_eq!(items2[0]["key"], "shannon1948");

    // Search by year
    let res3 = k
        .invoke(
            "citations.search",
            Payload::Json(json!({ "query": "1936" })),
        )
        .expect("search year 1936");
    let val3 = match res3 {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let items3 = val3["results"].as_array().expect("results array");
    assert!(!items3.is_empty());
    assert_eq!(items3[0]["key"], "turing1936");

    // Rejection of unknown arguments
    let err = k.invoke(
        "citations.search",
        Payload::Json(json!({ "query": "test", "extra_bad_arg": 123 })),
    );
    assert!(err.is_err(), "unknown arg must be rejected");
}

#[test]
fn extract_keys_across_syntaxes() {
    let mock = Arc::new(MockHttpClient::new());
    let k = test_kernel(mock);

    // Typst
    let typst_src = "As shown in @einstein1905 and later by @shannon1948[p. 12]. Contact author@mit.edu for details.";
    let res_typst = k
        .invoke(
            "citations.extract-keys",
            Payload::Json(json!({ "source": typst_src, "syntax": "typst" })),
        )
        .expect("extract typst");
    let val_typst = match res_typst {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let keys_typst: Vec<_> = val_typst["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(keys_typst, vec!["einstein1905", "shannon1948"]);

    // LaTeX
    let latex_src = r"\section{Intro} See \cite{einstein1905, shannon1948} and also \citep[see][p. 5]{turing1936}.";
    let res_latex = k
        .invoke(
            "citations.extract-keys",
            Payload::Json(json!({ "source": latex_src, "syntax": "latex" })),
        )
        .expect("extract latex");
    let val_latex = match res_latex {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let keys_latex: Vec<_> = val_latex["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        keys_latex,
        vec!["einstein1905", "shannon1948", "turing1936"]
    );

    // Markdown
    let md_src = "Discussion [@einstein1905; @shannon1948] and in-text @turing1936[p. 15].";
    let res_md = k
        .invoke(
            "citations.extract-keys",
            Payload::Json(json!({ "source": md_src, "syntax": "markdown" })),
        )
        .expect("extract md");
    let val_md = match res_md {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let keys_md: Vec<_> = val_md["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(keys_md, vec!["einstein1905", "shannon1948", "turing1936"]);
}

#[test]
fn format_apa_and_numeric_styles() {
    let mock = Arc::new(MockHttpClient::new());
    let k = test_kernel(mock);

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let bib_path = temp_dir.path().join("refs.bib");

    let bib = r#"
@article{watsoncrick1953,
    author = {James D. Watson and Francis H. C. Crick},
    title = {Molecular Structure of Nucleic Acids: A Structure for Deoxyribose Nucleic Acid},
    journal = {Nature},
    volume = {171},
    number = {4356},
    pages = {737--738},
    year = {1953},
    doi = {10.1038/171737a0}
}
"#;
    std::fs::write(&bib_path, bib).expect("write bib");
    k.invoke(
        "citations.load",
        Payload::Json(json!({ "path": bib_path.to_str().unwrap() })),
    )
    .expect("load");

    // Format APA
    let res_apa = k
        .invoke(
            "citations.format",
            Payload::Json(json!({ "key": "watsoncrick1953", "style": "apa" })),
        )
        .expect("format apa");
    let val_apa = match res_apa {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let formatted_apa = val_apa["formatted"].as_str().unwrap();
    assert!(formatted_apa.contains("Watson, J. D., & Crick, F. H. C."));
    assert!(formatted_apa.contains("Nature, 171(4356)"));
    assert!(formatted_apa.contains("737"));
    assert!(formatted_apa.contains("738"));
    assert!(formatted_apa.contains("https://doi.org/10.1038/171737a0"));

    // Format Numeric
    let res_num = k
        .invoke(
            "citations.format",
            Payload::Json(json!({ "key": "watsoncrick1953", "style": "numeric" })),
        )
        .expect("format numeric");
    let val_num = match res_num {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let formatted_num = val_num["formatted"].as_str().unwrap();
    assert!(formatted_num.starts_with("[1]"));
    assert!(formatted_num.contains("J. D. Watson and F. H. C. Crick"));
    assert!(formatted_num.contains("\"Molecular Structure of Nucleic Acids"));

    // The configured style applies when a request names none.
    let format_default = |k: &Kernel| match k
        .invoke(
            "citations.format",
            Payload::Json(json!({ "key": "watsoncrick1953" })),
        )
        .expect("format default")
    {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    let m = manifest().expect("manifest");
    assert_eq!(
        m.contributes.settings[0].default,
        Some(json!(manang_citations::DEFAULT_STYLE))
    );
    assert_eq!(format_default(&k)["formatted"], val_apa["formatted"]);
    k.invoke(
        "citations.configure",
        Payload::Json(json!({ "values": { "citations.style": "numeric" } })),
    )
    .expect("configure");
    let configured = format_default(&k);
    assert_eq!(configured["style"], "numeric");
    assert_eq!(configured["formatted"], val_num["formatted"]);
    // An explicit style still wins; unsupported styles are refused.
    let explicit = k
        .invoke(
            "citations.format",
            Payload::Json(json!({ "key": "watsoncrick1953", "style": "apa" })),
        )
        .expect("explicit");
    assert_eq!(explicit, Payload::Json(val_apa.clone()));
    for bad in [
        json!({ "values": { "citations.style": "chicago" } }),
        json!({ "values": { "citations.style": 3 } }),
        json!({}),
    ] {
        assert!(
            k.invoke("citations.configure", Payload::Json(bad.clone()))
                .is_err(),
            "{bad}"
        );
    }
    assert_eq!(format_default(&k)["style"], "numeric");
}

#[test]
fn verify_citation_offline_with_mock_http() {
    let mock = Arc::new(MockHttpClient::new());

    // Mock 1: Crossref DOI verification exact match
    let crossref_resp = json!({
        "status": "ok",
        "message": {
            "title": ["On the Electrodynamics of Moving Bodies"],
            "author": [
                { "family": "Einstein", "given": "Albert" }
            ],
            "issued": {
                "date-parts": [[1905, 9, 26]]
            },
            "container-title": ["Annalen der Physik"],
            "DOI": "10.1002/andp.19053221004"
        }
    });
    mock.set_json(
        "https://api.crossref.org/works/10.1002/andp.19053221004",
        200,
        &crossref_resp,
    );

    // Mock 2: DOI mismatch (different year)
    let mismatch_resp = json!({
        "status": "ok",
        "message": {
            "title": ["A Mathematical Theory of Communication"],
            "author": [
                { "family": "Shannon", "given": "Claude E." }
            ],
            "issued": {
                "date-parts": [[1950]] // Mismatched year (local is 1948)
            },
            "container-title": ["Bell System Technical Journal"],
            "DOI": "10.1002/j.1538-7305.1948.tb01338.x"
        }
    });
    mock.set_json(
        "https://api.crossref.org/works/10.1002/j.1538-7305.1948.tb01338.x",
        200,
        &mismatch_resp,
    );

    // Mock 3: arXiv Atom XML
    let arxiv_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2301.07041v1</id>
    <title>Quantum Error Correction for Classical Hardware</title>
    <author>
      <name>John Preskill</name>
    </author>
    <published>2023-01-17T18:00:00Z</published>
  </entry>
</feed>"#;
    mock.set_text(
        "https://export.arxiv.org/api/query?id_list=2301.07041",
        200,
        arxiv_xml,
    );

    let k = test_kernel(mock);

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let bib_path = temp_dir.path().join("verify_tests.bib");
    let bib = r#"
@article{einstein1905,
    author = {Albert Einstein},
    title = {On the Electrodynamics of Moving Bodies},
    year = {1905},
    doi = {10.1002/andp.19053221004}
}
@article{shannon1948,
    author = {Claude E. Shannon},
    title = {A Mathematical Theory of Communication},
    year = {1948},
    doi = {10.1002/j.1538-7305.1948.tb01338.x}
}
@article{preskill2023,
    author = {John Preskill},
    title = {Quantum Error Correction for Classical Hardware},
    year = {2023},
    arxiv = {2301.07041}
}
@article{notfound1999,
    author = {Ghost Author},
    title = {Nonexistent Paper Title That Will 404 Everywhere},
    doi = {10.9999/nonexistent.1234}
}
"#;
    std::fs::write(&bib_path, bib).expect("write bib");
    k.invoke(
        "citations.load",
        Payload::Json(json!({ "path": bib_path.to_str().unwrap() })),
    )
    .expect("load");

    // 1. Verify Einstein (exact match)
    let res1 = k
        .invoke(
            "citations.verify",
            Payload::Json(json!({ "key": "einstein1905" })),
        )
        .expect("verify einstein");
    let val1 = match res1 {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    assert_eq!(val1["status"], "verified");
    assert!(val1["differences"].as_array().unwrap().is_empty());
    assert!(val1["matched"].is_object());

    // 2. Verify Shannon (year mismatch)
    let res2 = k
        .invoke(
            "citations.verify",
            Payload::Json(json!({ "key": "shannon1948" })),
        )
        .expect("verify shannon");
    let val2 = match res2 {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    assert_eq!(val2["status"], "mismatch");
    let diffs = val2["differences"].as_array().unwrap();
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0]["field"], "year");
    assert_eq!(diffs[0]["local"], "1948");
    assert_eq!(diffs[0]["remote"], "1950");

    // 3. Verify arXiv (Preskill)
    let res3 = k
        .invoke(
            "citations.verify",
            Payload::Json(json!({ "key": "preskill2023" })),
        )
        .expect("verify preskill");
    let val3 = match res3 {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    assert_eq!(val3["status"], "verified");
    assert_eq!(val3["matched"]["journal_or_publisher"], "arXiv");

    // 4. Verify Not Found
    let res4 = k
        .invoke(
            "citations.verify",
            Payload::Json(json!({ "key": "notfound1999" })),
        )
        .expect("verify not found");
    let val4 = match res4 {
        Payload::Json(v) => v,
        _ => panic!("json"),
    };
    assert_eq!(val4["status"], "not_found");
}

#[test]
fn insta_snapshots_formatted_and_parse_errors() {
    // 1. Format snapshot
    let entry = Entry {
        key: "turing1936".to_string(),
        entry_type: "article".to_string(),
        title: Some(
            "On Computable Numbers, with an Application to the Entscheidungsproblem".to_string(),
        ),
        authors: vec![Author::with_parts("Turing", "Alan M.")],
        year: Some(1936),
        month: None,
        journal_or_publisher: Some("Proceedings of the London Mathematical Society".to_string()),
        volume: Some("s2-42".to_string()),
        number: Some("1".to_string()),
        pages: Some("230–265".to_string()),
        doi: Some("10.1112/plms/s2-42.1.230".to_string()),
        url: None,
        arxiv_id: None,
        abstract_text: None,
        raw_fields: std::collections::HashMap::new(),
    };

    let apa = manang_citations::format_apa(&entry);
    insta::assert_snapshot!("citation_format_apa", apa);

    let numeric = manang_citations::format_numeric(&entry);
    insta::assert_snapshot!("citation_format_numeric", numeric);

    // 2. BibTeX parse error with line numbers
    let malformed_bib =
        "@article{bad_key,\n  title = {Unclosed Title Brace\n  author = {Nobody}\n}";
    let bib_err = manang_citations::parse_bibtex(malformed_bib, Some("malformed.bib")).unwrap_err();
    insta::assert_debug_snapshot!("citation_bibtex_parse_error", bib_err);

    // 3. CSL-JSON parse error with line numbers
    let malformed_csl = "[\n  {\n    \"id\": \"invalid_json\",\n    \"title\": \"Missing Comma\"\n    \"year\": 2024\n  }\n]";
    let csl_err =
        manang_citations::parse_csl_json(malformed_csl, Some("malformed.json")).unwrap_err();
    insta::assert_debug_snapshot!("citation_csl_json_parse_error", csl_err);
}
