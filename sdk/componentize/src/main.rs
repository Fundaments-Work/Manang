//! `manang-componentize <core.wasm> <plugin.wasm>`
//!
//! Wraps the core module produced by `cargo build --target
//! wasm32-unknown-unknown` (which carries its WIT as a custom section, put
//! there by `wit-bindgen`) into a WebAssembly component. Run by
//! `scripts/build-plugin.sh`.

use std::process::ExitCode;

fn run(input: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let module = std::fs::read(input)?;
    let component = wit_component::ComponentEncoder::default()
        .validate(true)
        .module(&module)?
        .encode()?;
    std::fs::write(output, component)?;
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: manang-componentize <core.wasm> <plugin.wasm>");
        return ExitCode::from(2);
    };
    match run(input, output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("manang-componentize: {e}");
            ExitCode::FAILURE
        }
    }
}
