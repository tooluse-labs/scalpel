use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=SCALPEL_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=SCALPEL_RELEASE_DATE");
    println!("cargo:rerun-if-env-changed=SCALPEL_MUPDF_VERSION");
    println!("cargo:rerun-if-env-changed=MUPDF_VERSION");
    println!("cargo:rerun-if-env-changed=SCALPEL_MUPDF_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=SCALPEL_MUPDF_INCLUDE_DIR");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.join("../..");
    let version_file = workspace_root.join("third_party/mupdf.version");
    println!("cargo:rerun-if-changed={}", version_file.display());

    let commit = env_string("SCALPEL_BUILD_COMMIT")
        .or_else(|| git_output(&["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());
    let release_date = env_string("SCALPEL_RELEASE_DATE")
        .or_else(|| git_output(&["show", "-s", "--format=%cs", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());
    let mupdf_version = mupdf_version(&version_file);

    println!("cargo:rustc-env=SCALPEL_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=SCALPEL_RELEASE_DATE={release_date}");
    if let Some(mupdf_version) = mupdf_version {
        println!("cargo:rustc-env=SCALPEL_BUILD_MUPDF_VERSION={mupdf_version}");
    }

    embed_windows_app_resources(&manifest_dir);
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

fn mupdf_version(version_file: &Path) -> Option<String> {
    env_string("SCALPEL_MUPDF_VERSION")
        .or_else(read_mupdf_version_from_env_header)
        .or_else(|| env_string("MUPDF_VERSION"))
        .or_else(infer_mupdf_version_from_env_path)
        .or_else(|| read_mupdf_version_file(version_file))
}

fn read_mupdf_version_from_env_header() -> Option<String> {
    for path in [
        env::var_os("SCALPEL_MUPDF_SOURCE_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from),
        env::var_os("SCALPEL_MUPDF_INCLUDE_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        let version_header = if path.ends_with("include") {
            path.join("mupdf").join("fitz").join("version.h")
        } else {
            path.join("include")
                .join("mupdf")
                .join("fitz")
                .join("version.h")
        };
        if let Ok(header) = fs::read_to_string(version_header) {
            if let Some(version) = parse_mupdf_version_header(&header) {
                return Some(version);
            }
        }
    }
    None
}

fn parse_mupdf_version_header(header: &str) -> Option<String> {
    header.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("#define FZ_VERSION ")?;
        let version = rest.trim().strip_prefix('"')?.split('"').next()?;
        (!version.trim().is_empty()).then(|| version.to_string())
    })
}

fn infer_mupdf_version_from_env_path() -> Option<String> {
    [
        env::var_os("SCALPEL_MUPDF_SOURCE_DIR"),
        env::var_os("SCALPEL_MUPDF_INCLUDE_DIR"),
    ]
    .into_iter()
    .flatten()
    .map(PathBuf::from)
    .find_map(|path| infer_mupdf_version_from_path(&path))
}

fn infer_mupdf_version_from_path(path: &Path) -> Option<String> {
    path.ancestors()
        .filter_map(|ancestor| ancestor.file_name()?.to_str())
        .find_map(extract_mupdf_version_from_component)
}

fn extract_mupdf_version_from_component(component: &str) -> Option<String> {
    let rest = component.split_once("mupdf-")?.1;
    let version = rest
        .split_once("-source")
        .map_or(rest, |(version, _)| version)
        .trim();
    let looks_like_version =
        version.chars().all(|ch| ch.is_ascii_digit() || ch == '.') && version.contains('.');
    looks_like_version.then(|| version.to_string())
}

fn read_mupdf_version_file(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()?.lines().find_map(|line| {
        let version = line.trim().strip_prefix("MUPDF_VERSION=")?.trim();
        (!version.is_empty()).then(|| version.to_string())
    })
}

fn embed_windows_app_resources(manifest_dir: &Path) {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    let rc_path = manifest_dir.join("assets/windows/scalpel.rc");
    let icon_path = manifest_dir.join("assets/icons/scalpel.ico");
    println!("cargo:rerun-if-changed={}", rc_path.display());
    println!("cargo:rerun-if-changed={}", icon_path.display());

    if !icon_path.exists() {
        panic!("Windows app icon asset is missing: {}", icon_path.display());
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let res_path = out_dir.join("scalpel.res");
    match compile_windows_resource(manifest_dir, &rc_path, &res_path) {
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
