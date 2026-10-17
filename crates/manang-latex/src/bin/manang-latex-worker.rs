//! Out-of-process Tectonic runner. Usage: `manang-latex-worker <request.json>`.
//!
//! Exit status 0 means `outcome.json` was written (TeX errors included).

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(request) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: manang-latex-worker <request.json>");
        return ExitCode::from(2);
    };
    match manang_latex::worker::run_from_file(&request) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
