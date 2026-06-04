use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=include/pdbg_shim.h");
    println!("cargo:rerun-if-changed=c/pdbg_shim_fake.c");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let obj = out_dir.join("pdbg_shim_fake.o");
    let lib = out_dir.join("libpdbg_shim_fake.a");

    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let ar = env::var("AR").unwrap_or_else(|_| "ar".to_string());

    let mut cc_command = Command::new(&cc);
    cc_command.args([
        "-std=c11",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-Iinclude",
        "-c",
        "c/pdbg_shim_fake.c",
        "-o",
    ]);
    for flag in env::var("CFLAGS").unwrap_or_default().split_whitespace() {
        cc_command.arg(flag);
    }
    let cc_status = cc_command
        .arg(&obj)
        .status()
        .expect("failed to run C compiler");
    assert!(cc_status.success(), "C compiler failed");

    let ar_status = Command::new(&ar)
        .arg("crs")
        .arg(&lib)
        .arg(&obj)
        .status()
        .expect("failed to run ar");
    assert!(ar_status.success(), "ar failed");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=pdbg_shim_fake");
}
