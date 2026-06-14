use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const MUPDF_SETUP_HELP: &str = "\
Run `scripts/setup-mupdf.ps1` on Windows or `sh scripts/setup-mupdf.sh` \
on Unix from the repository root, then load the generated environment file; \
or set SCALPEL_MUPDF_SOURCE_DIR, SCALPEL_MUPDF_INCLUDE_DIR, and \
SCALPEL_MUPDF_LIB_DIR manually.";

fn main() {
    println!("cargo:rerun-if-changed=include/pdbg_shim.h");
    println!("cargo:rerun-if-env-changed=AR");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=CFLAGS");
    println!("cargo:rerun-if-env-changed=CXX");
    println!("cargo:rerun-if-env-changed=CXXFLAGS");

    if env::var_os("CARGO_FEATURE_REAL_MUPDF").is_some() {
        build_real_mupdf();
    } else {
        build_fake();
    }
}

fn build_fake() {
    println!("cargo:rerun-if-changed=c/pdbg_shim_fake.c");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let obj = out_dir.join(object_file_name("pdbg_shim_fake"));
    let lib = out_dir.join(static_library_file_name("pdbg_shim_fake"));

    compile_c_object("c/pdbg_shim_fake.c", &obj, &[]);
    archive_static_library(&lib, &[obj]);

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=pdbg_shim_fake");
}

fn build_real_mupdf() {
    println!("cargo:rerun-if-changed=c/pdbg_shim_mupdf.c");
    println!("cargo:rerun-if-env-changed=SCALPEL_MUPDF_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=SCALPEL_MUPDF_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=SCALPEL_MUPDF_LIB_DIR");
    println!("cargo:rerun-if-env-changed=SCALPEL_MUPDF_LIBS");
    println!("cargo:rerun-if-env-changed=SCALPEL_MUPDF_LINK_MODE");

    let source_dir = env::var_os("SCALPEL_MUPDF_SOURCE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let include_dir = env::var_os("SCALPEL_MUPDF_INCLUDE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| source_dir.as_ref().map(|dir| dir.join("include")))
        .unwrap_or_else(|| {
            panic!(
                "real-mupdf requires SCALPEL_MUPDF_SOURCE_DIR or SCALPEL_MUPDF_INCLUDE_DIR.\n{}",
                MUPDF_SETUP_HELP
            )
        });
    if !include_dir.is_dir() {
        panic!(
            "MuPDF include directory does not exist: {}\n{}",
            include_dir.display(),
            MUPDF_SETUP_HELP
        );
    }
    if !include_dir.join("mupdf").is_dir() {
        panic!(
            "MuPDF include directory does not contain a mupdf/ subdirectory: {}\n{}",
            include_dir.display(),
            MUPDF_SETUP_HELP
        );
    }

    let lib_dir = env::var_os("SCALPEL_MUPDF_LIB_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            source_dir.as_ref().map(|dir| {
                if target_is_msvc() {
                    dir.join("platform")
                        .join("win32")
                        .join(msvc_mupdf_platform_dir())
                        .join("Release")
                } else {
                    dir.join("build").join("release")
                }
            })
        })
        .unwrap_or_else(|| {
            panic!(
                "real-mupdf requires SCALPEL_MUPDF_LIB_DIR or SCALPEL_MUPDF_SOURCE_DIR.\n{}",
                MUPDF_SETUP_HELP
            )
        });
    if !lib_dir.is_dir() {
        panic!(
            "MuPDF library directory does not exist: {}\n{}",
            lib_dir.display(),
            MUPDF_SETUP_HELP
        );
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let obj = out_dir.join(object_file_name("pdbg_shim_mupdf"));
    let lib = out_dir.join(static_library_file_name("pdbg_shim_mupdf"));

    compile_c_object("c/pdbg_shim_mupdf.c", &obj, &[include_dir]);
    archive_static_library(&lib, &[obj]);

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=pdbg_shim_mupdf");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    let msvc_compat_lib = if target_is_msvc() {
        source_dir
            .as_ref()
            .map(|dir| build_msvc_mupdf_compat(dir, &out_dir))
    } else {
        None
    };

    let link_mode = env_string("SCALPEL_MUPDF_LINK_MODE").unwrap_or_else(|| {
        if target_is_msvc() {
            "static:+whole-archive".to_string()
        } else {
            "static".to_string()
        }
    });
    let libs = env_string("SCALPEL_MUPDF_LIBS").unwrap_or_else(|| {
        if target_is_msvc() {
            "libmupdf".to_string()
        } else {
            "mupdf,mupdf-third".to_string()
        }
    });
    for lib_name in libs
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .filter(|name| !name.is_empty())
    {
        println!("cargo:rustc-link-lib={}={}", link_mode, lib_name);
    }
    if let Some(compat_lib) = msvc_compat_lib {
        println!("cargo:rustc-link-lib=static:+whole-archive={}", compat_lib);
    }
    if !target_is_msvc() {
        println!("cargo:rustc-link-lib=pthread");
    }
}

fn env_string(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn build_msvc_mupdf_compat(source_dir: &Path, out_dir: &Path) -> &'static str {
    let ccthin_source = source_dir
        .join("thirdparty")
        .join("leptonica")
        .join("src")
        .join("ccthin.c");
    let gsubgpos_context_source = source_dir
        .join("thirdparty")
        .join("harfbuzz")
        .join("src")
        .join("graph")
        .join("gsubgpos-context.cc");
    println!("cargo:rerun-if-changed={}", ccthin_source.display());
    println!(
        "cargo:rerun-if-changed={}",
        gsubgpos_context_source.display()
    );

    let ccthin_obj = out_dir.join(object_file_name("pdbg_mupdf_ccthin"));
    let gsubgpos_context_obj = out_dir.join(object_file_name("pdbg_mupdf_gsubgpos_context"));

    compile_msvc_leptonica_ccthin(source_dir, &ccthin_source, &ccthin_obj);
    compile_msvc_harfbuzz_gsubgpos_context(
        source_dir,
        &gsubgpos_context_source,
        &gsubgpos_context_obj,
    );

    let lib_name = "pdbg_mupdf_win_compat";
    let lib = out_dir.join(static_library_file_name(lib_name));
    archive_static_library(&lib, &[ccthin_obj, gsubgpos_context_obj]);
    lib_name
}

fn target_is_msvc() -> bool {
    env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
}

fn msvc_mupdf_platform_dir() -> &'static str {
    match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "Win32",
        Ok("aarch64") => "ARM64",
        _ => "x64",
    }
}

fn object_file_name(name: &str) -> String {
    if target_is_msvc() {
        format!("{name}.obj")
    } else {
        format!("{name}.o")
    }
}

fn static_library_file_name(name: &str) -> String {
    if target_is_msvc() {
        format!("{name}.lib")
    } else {
        format!("lib{name}.a")
    }
}

fn compile_c_object(source: &str, obj: &Path, include_dirs: &[PathBuf]) {
    let cc = env::var("CC").unwrap_or_else(|_| {
        if target_is_msvc() {
            "cl.exe".to_string()
        } else {
            "cc".to_string()
        }
    });

    let mut cc_command = Command::new(&cc);
    if target_is_msvc() {
        let external_headers = !include_dirs.is_empty();
        cc_command.args([
            "/nologo",
            "/std:c11",
            "/experimental:c11atomics",
            "/D_CRT_SECURE_NO_WARNINGS",
            "/D_CRT_NONSTDC_NO_WARNINGS",
            "/W4",
            "/Iinclude",
        ]);
        if !external_headers {
            cc_command.arg("/WX");
        }
        for include_dir in include_dirs {
            cc_command.arg(format!("/I{}", include_dir.display()));
        }
        for flag in env::var("CFLAGS").unwrap_or_default().split_whitespace() {
            cc_command.arg(flag);
        }
        cc_command.args(["/c", source]);
        cc_command.arg(format!("/Fo{}", obj.display()));
    } else {
        cc_command.args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-Iinclude"]);
        for include_dir in include_dirs {
            cc_command.arg("-I").arg(include_dir);
        }
        for flag in env::var("CFLAGS").unwrap_or_default().split_whitespace() {
            cc_command.arg(flag);
        }
        cc_command.args(["-c", source, "-o"]);
        cc_command.arg(obj);
    }
    let cc_status = cc_command.status().expect("failed to run C compiler");
    assert!(cc_status.success(), "C compiler failed");
}

fn compile_msvc_leptonica_ccthin(source_dir: &Path, source: &Path, obj: &Path) {
    let cc = env::var("CC").unwrap_or_else(|_| "cl.exe".to_string());
    let mut cc_command = Command::new(&cc);
    cc_command.args([
        "/nologo",
        "/c",
        "/Zi",
        "/W3",
        "/WX-",
        "/diagnostics:column",
        "/sdl-",
        "/O2",
        "/Oi",
        "/DHAVE_LEPTONICA",
        "/DLEPTONICA_INTERCEPT_ALLOC=1",
        "/DHAVE_LIBPNG=0",
        "/DHAVE_LIBTIFF=0",
        "/DHAVE_LIBJPEG=0",
        "/DHAVE_LIBZ=0",
        "/DHAVE_LIBGIF=0",
        "/DHAVE_LIBUNGIF=0",
        "/DHAVE_LIBWEBP=0",
        "/DHAVE_LIBWEBP_ANIM=0",
        "/DHAVE_LIBJP2K=0",
        "/D_CRT_SECURE_NO_WARNINGS",
        "/DNDEBUG",
        "/D_LIB",
        "/D_UNICODE",
        "/UUNICODE",
        "/Gm-",
        "/EHsc",
        "/MD",
        "/GS",
        "/Gy",
        "/fp:precise",
        "/Zc:wchar_t",
        "/Zc:forScope",
        "/Zc:inline",
        "/permissive-",
        "/external:W3",
        "/Gd",
        "/TC",
        "/wd4018",
        "/wd4100",
        "/wd4101",
        "/wd4244",
        "/wd4200",
        "/wd4267",
        "/wd4305",
        "/FC",
    ]);
    for include_dir in [
        source_dir.join("include"),
        source_dir.join("thirdparty").join("leptonica").join("src"),
        source_dir.join("scripts").join("tesseract"),
    ] {
        cc_command.arg(format!("/I{}", include_dir.display()));
    }
    for flag in env::var("CFLAGS").unwrap_or_default().split_whitespace() {
        cc_command.arg(flag);
    }
    cc_command.arg(format!("/Fo{}", obj.display()));
    cc_command.arg(format!(
        "/Fd{}",
        obj.with_file_name("pdbg_mupdf_win_compat.pdb").display()
    ));
    cc_command.arg(source);
    let status = cc_command
        .status()
        .expect("failed to run MSVC C compiler for MuPDF compatibility source");
    assert!(
        status.success(),
        "MSVC C compiler failed for {}",
        source.display()
    );
}

fn compile_msvc_harfbuzz_gsubgpos_context(source_dir: &Path, source: &Path, obj: &Path) {
    let cxx = env::var("CXX").unwrap_or_else(|_| "cl.exe".to_string());
    let mut cxx_command = Command::new(&cxx);
    cxx_command.args([
        "/nologo",
        "/c",
        "/Zi",
        "/W3",
        "/WX-",
        "/diagnostics:column",
        "/O2",
        "/Oi",
        "/DWIN64",
        "/DNDEBUG",
        "/D_CRT_SECURE_NO_WARNINGS",
        "/DHB_NO_MT",
        "/Dhb_malloc_impl=fz_hb_malloc",
        "/Dhb_calloc_impl=fz_hb_calloc",
        "/Dhb_realloc_impl=fz_hb_realloc",
        "/Dhb_free_impl=fz_hb_free",
        "/DHAVE_FREETYPE",
        "/D_MBCS",
        "/Gm-",
        "/EHsc",
        "/MD",
        "/GS",
        "/Gy",
        "/fp:precise",
        "/Zc:wchar_t",
        "/Zc:forScope",
        "/Zc:inline",
        "/external:W3",
        "/Gd",
        "/TP",
        "/wd4018",
        "/wd4244",
        "/wd4146",
        "/wd4267",
        "/FC",
    ]);
    let freetype_include = source_dir
        .join("thirdparty")
        .join("freetype")
        .join("include");
    cxx_command.arg(format!("/I{}", freetype_include.display()));
    for flag in env::var("CXXFLAGS").unwrap_or_default().split_whitespace() {
        cxx_command.arg(flag);
    }
    cxx_command.arg(format!("/Fo{}", obj.display()));
    cxx_command.arg(format!(
        "/Fd{}",
        obj.with_file_name("pdbg_mupdf_win_compat.pdb").display()
    ));
    cxx_command.arg(source);
    let status = cxx_command
        .status()
        .expect("failed to run MSVC C++ compiler for MuPDF compatibility source");
    assert!(
        status.success(),
        "MSVC C++ compiler failed for {}",
        source.display()
    );
}

fn archive_static_library(lib: &Path, objs: &[PathBuf]) {
    let ar = env::var("AR").unwrap_or_else(|_| {
        if target_is_msvc() {
            "lib.exe".to_string()
        } else {
            "ar".to_string()
        }
    });
    let mut ar_command = Command::new(&ar);
    if target_is_msvc() {
        ar_command.arg("/NOLOGO");
        ar_command.arg(format!("/OUT:{}", lib.display()));
        ar_command.args(objs);
    } else {
        ar_command.arg("crs").arg(lib).args(objs);
    }
    let ar_status = ar_command.status().expect("failed to run ar");
    assert!(ar_status.success(), "ar failed");
}
