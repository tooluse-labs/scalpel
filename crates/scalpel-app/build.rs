use std::env;
use std::path::{Path, PathBuf};
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

    embed_windows_app_resources();
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

fn embed_windows_app_resources() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let rc_path = manifest_dir.join("assets/windows/scalpel.rc");
    let icon_path = manifest_dir.join("assets/icons/scalpel.ico");
    println!("cargo:rerun-if-changed={}", rc_path.display());
    println!("cargo:rerun-if-changed={}", icon_path.display());

    if !icon_path.exists() {
        panic!("Windows app icon asset is missing: {}", icon_path.display());
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let res_path = out_dir.join("scalpel.res");
    match compile_windows_resource(&manifest_dir, &rc_path, &res_path) {
        Ok(()) => {
            println!("cargo:rustc-link-arg-bin=scalpel={}", res_path.display());
        }
        Err(err) if env::var("PROFILE").as_deref() != Ok("release") => {
            println!("cargo:warning=skipping Windows icon resource: {err}");
        }
        Err(err) => {
            panic!("failed to embed Windows icon resource: {err}");
        }
    }
}

fn compile_windows_resource(
    manifest_dir: &Path,
    rc_path: &Path,
    res_path: &Path,
) -> Result<(), String> {
    let status = Command::new("rc.exe")
        .current_dir(manifest_dir)
        .arg("/nologo")
        .arg(format!("/fo{}", res_path.display()))
        .arg(rc_path)
        .status()
        .map_err(|err| format!("could not run rc.exe: {err}"))?;
    if !status.success() {
        return Err(format!("rc.exe exited with status {status}"));
    }
    Ok(())
}
