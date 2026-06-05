use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=include/pdbg_shim.h");
    println!("cargo:rerun-if-env-changed=AR");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=CFLAGS");

    if env::var_os("CARGO_FEATURE_REAL_MUPDF").is_some() {
        build_real_mupdf();
    } else {
        build_fake();
    }
}

fn build_fake() {
    println!("cargo:rerun-if-changed=c/pdbg_shim_fake.c");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let obj = out_dir.join("pdbg_shim_fake.o");
    let lib = out_dir.join("libpdbg_shim_fake.a");

    compile_c_object("c/pdbg_shim_fake.c", &obj, &[]);
    archive_static_library(&lib, &obj);

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=pdbg_shim_fake");
}

fn build_real_mupdf() {
    println!("cargo:rerun-if-changed=c/pdbg_shim_mupdf.c");
    println!("cargo:rerun-if-env-changed=PDBG_MUPDF_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=PDBG_MUPDF_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=PDBG_MUPDF_LIB_DIR");
    println!("cargo:rerun-if-env-changed=PDBG_MUPDF_LIBS");
    println!("cargo:rerun-if-env-changed=PDBG_MUPDF_LINK_MODE");

    let source_dir = env::var_os("PDBG_MUPDF_SOURCE_DIR").map(PathBuf::from);
    let include_dir = env::var_os("PDBG_MUPDF_INCLUDE_DIR")
        .map(PathBuf::from)
        .or_else(|| source_dir.as_ref().map(|dir| dir.join("include")))
        .expect(
            "real-mupdf requires PDBG_MUPDF_SOURCE_DIR or PDBG_MUPDF_INCLUDE_DIR; \
             point it at the pinned MuPDF source tree selected in ADR 0002",
        );
    if !include_dir.join("mupdf").is_dir() {
        panic!(
            "MuPDF include directory does not contain a mupdf/ subdirectory: {}",
            include_dir.display()
        );
    }

    let lib_dir = env::var_os("PDBG_MUPDF_LIB_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            source_dir
                .as_ref()
                .map(|dir| dir.join("build").join("release"))
        })
        .expect(
            "real-mupdf requires PDBG_MUPDF_LIB_DIR or PDBG_MUPDF_SOURCE_DIR; \
             build MuPDF first so libmupdf is available",
        );

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let obj = out_dir.join("pdbg_shim_mupdf.o");
    let lib = out_dir.join("libpdbg_shim_mupdf.a");

    compile_c_object("c/pdbg_shim_mupdf.c", &obj, &[include_dir]);
    archive_static_library(&lib, &obj);

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=pdbg_shim_mupdf");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    let link_mode = env::var("PDBG_MUPDF_LINK_MODE").unwrap_or_else(|_| "static".to_string());
    let libs = env::var("PDBG_MUPDF_LIBS").unwrap_or_else(|_| "mupdf,mupdf-third".to_string());
    for lib_name in libs
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .filter(|name| !name.is_empty())
    {
        println!("cargo:rustc-link-lib={}={}", link_mode, lib_name);
    }
    println!("cargo:rustc-link-lib=pthread");
}

fn compile_c_object(source: &str, obj: &Path, include_dirs: &[PathBuf]) {
    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());

    let mut cc_command = Command::new(&cc);
    cc_command.args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-Iinclude"]);
    for include_dir in include_dirs {
        cc_command.arg("-I").arg(include_dir);
    }
    for flag in env::var("CFLAGS").unwrap_or_default().split_whitespace() {
        cc_command.arg(flag);
    }
    cc_command.args(["-c", source, "-o"]);
    let cc_status = cc_command
        .arg(obj)
        .status()
        .expect("failed to run C compiler");
    assert!(cc_status.success(), "C compiler failed");
}

fn archive_static_library(lib: &Path, obj: &Path) {
    let ar = env::var("AR").unwrap_or_else(|_| "ar".to_string());
    let ar_status = Command::new(&ar)
        .arg("crs")
        .arg(lib)
        .arg(obj)
        .status()
        .expect("failed to run ar");
    assert!(ar_status.success(), "ar failed");
}
