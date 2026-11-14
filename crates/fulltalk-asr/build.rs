//! macOS: the prebuilt ONNX Runtime uses `@available` checks, which call
//! `__isPlatformVersionAtLeast` from clang's compiler-rt. rustc does not link
//! compiler-rt on Apple targets, so release (LTO) links fail without this.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(output) = Command::new("clang").arg("--print-resource-dir").output() else {
        println!("cargo:warning=clang not found; skipping compiler-rt link");
        return;
    };
    let resource_dir = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    println!("cargo:rustc-link-search=native={resource_dir}/lib/darwin");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
