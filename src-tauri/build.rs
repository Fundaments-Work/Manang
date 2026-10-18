use std::process::Command;

fn main() {
    // The commit the build was made from, for About and the diagnostics report.
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=MANANG_GIT_COMMIT={commit}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    // `MANANG_CHANNEL=beta` marks a beta build; anything else is a dev build.
    println!("cargo:rerun-if-env-changed=MANANG_CHANNEL");
    tauri_build::build()
}
