//! Request-body comparison: byte level, semantic level, and a list of differing JSON paths.
#![allow(dead_code)]

use serde_json::Value;

#[derive(Debug)]
pub struct BodyDiff {
    pub identical_bytes: bool,
    pub equal_json: bool,
    /// Paths (JSON-pointer-like) where the two documents differ, with both values.
    pub differences: Vec<String>,
    /// Whether the top-level key order differs.
    pub key_order_differs: bool,
}

fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => walk(&format!("{path}/{k}"), v, w, out),
                    None => out.push(format!("{path}/{k}: only in-house = {v}")),
                }
            }
            for (k, w) in y {
                if !x.contains_key(k) {
                    out.push(format!("{path}/{k}: only rig = {w}"));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                walk(&format!("{path}/{i}"), v, w, out);
            }
        }
        _ => {
            if a != b {
                out.push(format!("{path}: in-house = {a} | rig = {b}"));
            }
        }
    }
}

fn keys(v: &Value) -> Vec<String> {
    v.as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn diff(inhouse: &[u8], rig: &[u8]) -> BodyDiff {
    let a: Value = serde_json::from_slice(inhouse).expect("in-house body is JSON");
    let b: Value = serde_json::from_slice(rig).expect("rig body is JSON");
    let mut differences = Vec::new();
    walk("", &a, &b, &mut differences);
    BodyDiff {
        identical_bytes: inhouse == rig,
        equal_json: a == b,
        differences,
        key_order_differs: keys(&a) != keys(&b),
    }
}

pub fn report(name: &str, inhouse: &[u8], rig: &[u8]) -> BodyDiff {
    let d = diff(inhouse, rig);
    println!("==== {name}");
    println!("bytes identical: {}", d.identical_bytes);
    println!("parsed JSON equal: {}", d.equal_json);
    println!("top-level key order differs: {}", d.key_order_differs);
    println!("in-house: {}", String::from_utf8_lossy(inhouse));
    println!("rig     : {}", String::from_utf8_lossy(rig));
    for line in &d.differences {
        println!("  diff {line}");
    }
    d
}
