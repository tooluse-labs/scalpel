use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=SCALPEL_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=SCALPEL_RELEASE_DATE");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");

    let commit = env_string("SCALPEL_BUILD_COMMIT")
        .or_else(|| git_output(&["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());
    let release_date = env_string("SCALPEL_RELEASE_DATE")
        .or_else(|| git_output(&["show", "-s", "--format=%cs", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=SCALPEL_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=SCALPEL_RELEASE_DATE={release_date}");
}

fn env_string(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
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
