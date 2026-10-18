//! Formulas end to end, through the kernel with the real Typst plugin: an
//! agent adds Typst and LaTeX math, the canvas asks for renderings, export
//! draws vector paths in the node's colour, and mistakes come back as
//! positioned errors. Also colours, backgrounds and the diamond label.

use std::sync::Arc;

use manang_figure::FigureScene;
use manang_kernel::{EventBus, Kernel, Payload};
use manang_typst::CompilerConfig;
use serde_json::{Value, json};

fn kernel() -> Kernel {
    let k = Kernel::new();
    k.register(
        manang_figure::manifest().expect("manifest"),
        Arc::new(manang_figure::plugin_with_events(EventBus::new())),
    )
    .expect("figure");
    k.register(
        manang_typst::manifest().expect("manifest"),
        Arc::new(manang_typst::plugin(CompilerConfig::default()).expect("typst")),
    )
    .expect("typst");
    k
}

fn call(k: &Kernel, tool: &str, input: Value) -> Value {
    match k.call_agent_tool(tool, input.clone()) {
        Ok(Payload::Json(v)) => v,
        other => panic!("{tool} {input}: {other:?}"),
    }
}

fn svg_of(k: &Kernel, command: &str, args: Value) -> String {
    match k.invoke(command, Payload::Json(args)) {
        Ok(Payload::Binary(bytes)) => String::from_utf8(bytes.to_vec()).expect("utf8"),
        other => panic!("{command}: {other:?}"),
    }
}

fn new_figure(k: &Kernel, dir: &std::path::Path) -> (u64, String) {
    let path = dir.join("math.figure.json");
    let made = k
        .invoke(
            "figure.create",
            Payload::Json(json!({ "path": path.to_string_lossy(), "width": 640, "height": 360 })),
        )
        .expect("create");
    let Payload::Json(v) = made else {
        panic!("json")
    };
    (
        v["id"].as_u64().expect("id"),
        path.to_string_lossy().into_owned(),
    )
}

#[test]
fn an_agent_adds_formulas_in_both_languages_and_exports_vector_paths() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel();
    let (id, path) = new_figure(&k, dir.path());
    let added = call(
        &k,
        "figure_add_shapes",
        json!({ "path": path, "shapes": [
            { "kind": "formula", "source": "integral_0^1 x^2 dif x = 1/3", "x": 40, "y": 40, "size": 28 },
            { "kind": "formula", "language": "latex", "source": "\\sum_{k=1}^{n} k = \\frac{n(n+1)}{2}",
              "x": 40, "y": 160, "style": { "stroke": "#0072b2" } },
            { "kind": "rect", "label": "Result", "x": 400, "y": 80 }
        ]}),
    );
    let ids: Vec<&str> = added["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(ids.len(), 3);
    // Both formulas were measured by their renderers, not estimated.
    let shapes = added["shapes"].as_array().expect("shapes");
    for s in &shapes[..2] {
        assert!(s["width"].as_f64().expect("w") > 30.0, "{s}");
        assert!(s["height"].as_f64().expect("h") > 10.0, "{s}");
    }
    // An arrow binds to a formula like to any shape.
    call(
        &k,
        "figure_connect",
        json!({ "path": path, "connections": [{ "from": ids[0], "to": ids[2], "label": "gives" }] }),
    );

    let described = call(&k, "figure_describe", json!({ "path": path }));
    let other = described["other"].as_array().expect("other");
    assert_eq!(other[0]["language"], "typst");
    assert_eq!(other[1]["language"], "latex");
    assert_eq!(other[1]["source"], "\\sum_{k=1}^{n} k = \\frac{n(n+1)}{2}");

    let svg = match k
        .invoke("figure.export-svg", Payload::Json(json!({ "id": id })))
        .expect("export")
    {
        Payload::Binary(b) => String::from_utf8(b.to_vec()).expect("utf8"),
        other => panic!("{other:?}"),
    };
    assert!(
        !svg.contains("<text x=\"40\""),
        "formulas are paths, not text"
    );
    assert!(
        svg.contains("color=\"#0072b2\""),
        "custom colour reaches the paths"
    );
    assert!(svg.contains("fill=\"currentColor\""));
    // The export is a valid document for the other consumers too: Typst's
    // own SVG importer draws it into a PDF.
    let pdf = k
        .invoke(
            "typst.svg-to-pdf",
            Payload::Binary(Arc::from(svg.clone().into_bytes())),
        )
        .expect("the figure with formulas converts to PDF");
    assert!(matches!(pdf, Payload::Binary(b) if b.starts_with(b"%PDF")));
    insta::assert_json_snapshot!(
        "formula_describe",
        json!({ "other": described["other"], "summary": described["summary"] })
    );
    insta::assert_snapshot!("formula_export", svg);

    // The file keeps the source and language, not the rendering.
    k.invoke("figure.save", Payload::Json(json!({ "id": id })))
        .expect("save");
    let saved = std::fs::read_to_string(&path).expect("saved");
    let scene = FigureScene::from_json(&saved).expect("reads");
    assert!(!saved.contains("<svg"));
    assert!(saved.contains("\"source\": \"integral_0^1 x^2 dif x = 1/3\""));
    assert!(saved.contains("\"language\": \"latex\""));
    assert_eq!(scene.nodes.len(), 4);
}

#[test]
fn updating_a_formula_re_measures_it_and_mistakes_are_positioned() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel();
    let (_, path) = new_figure(&k, dir.path());
    let added = call(
        &k,
        "figure_add_shapes",
        json!({ "path": path, "shapes": [{ "kind": "formula", "source": "x", "x": 10, "y": 10 }] }),
    );
    let id = added["ids"][0].as_str().expect("id").to_owned();
    let before = added["shapes"][0]["width"].as_f64().expect("w");
    call(
        &k,
        "figure_update",
        json!({ "path": path, "updates": [{ "ref": id, "source": "x_1 + x_2 + x_3 + x_4", "language": "latex" }] }),
    );
    let described = call(&k, "figure_describe", json!({ "path": path }));
    let now = described["other"][0]["width"].as_f64().expect("w");
    assert!(now > before * 2.0, "{before} -> {now}");
    assert_eq!(described["other"][0]["language"], "latex");

    // A formula that does not compile is refused with a position.
    let err = k
        .call_agent_tool(
            "figure_add_shapes",
            json!({ "path": path, "shapes": [{ "kind": "formula", "source": "a +\nb + nosuch(3)" }] }),
        )
        .expect_err("bad formula");
    let text = err.to_string();
    assert!(
        text.contains("[math_error]") && text.contains("line 2"),
        "{text}"
    );
    let err = k
        .call_agent_tool(
            "figure_add_shapes",
            json!({ "path": path, "shapes": [{ "kind": "formula", "language": "latex", "source": "\\frac{1" }] }),
        )
        .expect_err("bad latex");
    assert!(err.to_string().contains("[math_error]"), "{err}");
    let err = k
        .call_agent_tool(
            "figure_add_shapes",
            json!({ "path": path, "shapes": [{ "kind": "formula" }] }),
        )
        .expect_err("no source");
    assert!(err.to_string().contains("source"), "{err}");
}

#[test]
fn the_canvas_command_renders_either_language_as_binary_svg() {
    let k = kernel();
    let typst = svg_of(
        &k,
        "figure.render-math",
        json!({ "source": "a^2 + b^2 = c^2", "size": 20 }),
    );
    let latex = svg_of(
        &k,
        "figure.render-math",
        json!({ "language": "latex", "source": "a^2 + b^2 = c^2", "size": 20 }),
    );
    for svg in [&typst, &latex] {
        assert!(svg.starts_with("<svg") && svg.contains("viewBox=\"0 0 "));
        assert!(!svg.contains("<text"), "outlines only");
        assert!(svg.contains("currentColor"));
    }
    // The canvas can ask for a colour baked in (an <image> cannot inherit one).
    let blue = svg_of(
        &k,
        "figure.render-math",
        json!({ "language": "latex", "source": "x^2", "size": 20, "color": "#0072B2" }),
    );
    assert!(blue.contains("#0072b2") && !blue.contains("currentColor"));
    assert!(
        k.invoke(
            "figure.render-math",
            Payload::Json(json!({ "source": "x", "color": "red" }))
        )
        .is_err()
    );
    // Cached or not, the same input answers with the same bytes.
    assert_eq!(
        typst,
        svg_of(
            &k,
            "figure.render-math",
            json!({ "source": "a^2 + b^2 = c^2", "size": 20 })
        )
    );
    let err = k
        .invoke(
            "figure.render-math",
            Payload::Json(json!({ "language": "latex", "source": "\\frac{1", "size": 20 })),
        )
        .expect_err("bad");
    assert!(err.to_string().contains("[math_error]"));
}

#[test]
fn colours_backgrounds_and_diamond_labels_export() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel();
    let (id, path) = new_figure(&k, dir.path());
    call(
        &k,
        "figure_add_shapes",
        json!({ "path": path, "shapes": [
            { "kind": "rect", "label": "Custom", "x": 40, "y": 40, "width": 160, "height": 80,
              "style": { "stroke": "#C6F135", "fill": "#102030", "fillOpacity": 40, "labelColor": "#FF8800" } },
            { "kind": "diamond", "label": "Validate input", "x": 260, "y": 20, "width": 150, "height": 110 }
        ]}),
    );
    k.invoke(
        "figure.apply",
        Payload::Json(json!({ "id": id, "op": { "op": "set-canvas", "width": 640, "height": 360, "background": "#FFFFFF" } })),
    )
    .expect("background");
    let svg = svg_of(&k, "figure.export-svg", json!({ "id": id }));
    // Hex literals are stored and drawn as lowercase #rrggbb.
    assert!(svg.contains("stroke=\"#c6f135\"") && svg.contains("fill=\"#ff8800\""));
    assert!(svg.contains("fill-opacity=\"0.4\""));
    assert!(svg.contains("<rect x=\"0\" y=\"0\" width=\"640\" height=\"360\" fill=\"#ffffff\"/>"));
    assert!(
        svg.contains(">Validate</tspan>") && svg.contains(">input</tspan>"),
        "the diamond label breaks at the space, not inside a word"
    );
    insta::assert_snapshot!("colours_background_diamond", svg);

    let scene = match k
        .invoke("figure.get", Payload::Json(json!({ "id": id })))
        .expect("get")
    {
        Payload::Json(v) => v,
        other => panic!("{other:?}"),
    };
    assert_eq!(scene["background"], "#ffffff");
    assert_eq!(scene["nodes"][0]["style"]["stroke"], "#c6f135");
}

#[test]
fn set_colour_undoes_in_one_step_and_restores_exactly() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel();
    let (id, path) = new_figure(&k, dir.path());
    call(
        &k,
        "figure_add_shapes",
        json!({ "path": path, "shapes": [
            { "label": "A", "x": 0, "y": 0 }, { "label": "B", "x": 200, "y": 0 },
            { "kind": "formula", "source": "x", "x": 0, "y": 200 }
        ]}),
    );
    let get = || match k
        .invoke("figure.get", Payload::Json(json!({ "id": id })))
        .expect("get")
    {
        Payload::Json(v) => v,
        other => panic!("{other:?}"),
    };
    let before = get();
    let ids: Vec<String> = before["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|n| n["id"].as_str().expect("id").to_owned())
        .collect();
    k.invoke(
        "figure.apply",
        Payload::Json(json!({ "id": id, "op": { "op": "set-style", "ids": ids,
            "style": { "stroke": "#ABCDEF", "labelColor": "#123456" } } })),
    )
    .expect("colour all three");
    let during = get();
    assert_eq!(during["nodes"][2]["style"]["stroke"], "#abcdef");
    k.invoke(
        "figure.apply",
        Payload::Json(json!({ "id": id, "op": { "op": "undo" } })),
    )
    .expect("undo");
    assert_eq!(get(), before, "one undo restores every node exactly");
    let bad = k.invoke(
        "figure.apply",
        Payload::Json(json!({ "id": id, "op": { "op": "set-style", "ids": ids,
            "style": { "stroke": "#12" } } })),
    );
    assert!(bad.is_err(), "a malformed hex is refused");
    assert_eq!(get(), before, "and changes nothing");
}
