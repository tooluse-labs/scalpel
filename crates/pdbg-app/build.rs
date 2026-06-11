use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=PDBG_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=PDBG_RELEASE_DATE");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");

    let commit = env::var("PDBG_BUILD_COMMIT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| git_output(&["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());
    let release_date = env::var("PDBG_RELEASE_DATE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| git_output(&["show", "-s", "--format=%cs", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=PDBG_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=PDBG_RELEASE_DATE={release_date}");
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}
