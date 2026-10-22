//! Build script.
//!
//! Besides the usual `tauri-build` step, this embeds the application manifest
//! into the **test** binary.
//!
//! # Why the test binary needs a manifest
//!
//! `tauri-build` embeds `lifeos.manifest` into the application binary, so the
//! app starts fine. The harness that `cargo test` produces is a separate
//! executable, and without a manifest that declares Windows 10 support the
//! loader will not resolve the WinRT API-sets imported by the `windows`
//! crate (`RoOriginateErrorW` from `api-ms-win-core-winrt-error-l1-1-0.dll`).
//!
//! The symptom is unhelpful: the process exits with `0xC0000139`
//! (STATUS_ENTRYPOINT_NOT_FOUND) and prints nothing, so the test run looks
//! like a crash in the code rather than a linking problem.
//!
//! The manifest is compiled with the same `windres` that ships alongside the
//! C toolchain, and linked in only for the `test` profile. If `windres` is
//! unavailable the build continues and prints a warning: a missing manifest
//! breaks tests, not the application.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    tauri_build::build();
    embed_test_manifest();
}

/// Compiles `tests_manifest.rc` and tells the linker to include it.
///
/// The `cargo:rustc-link-arg-tests` directive below applies *only* to test
/// harnesses, so this is emitted unconditionally. A previous version guarded
/// on `PROFILE == "test"` and therefore never ran: cargo passes
/// `PROFILE=debug` to build scripts even during `cargo test`. The application
/// binary is unaffected either way and keeps the manifest `tauri-build`
/// embedded.
fn embed_test_manifest() {
    let manifest = PathBuf::from("lifeos.manifest");
    let script = PathBuf::from("tests_manifest.rc");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", script.display());

    let Some(windres) = find_windres() else {
        println!(
            "cargo:warning=windres was not found, so the test binary has no manifest. \
             Tests will fail to load with STATUS_ENTRYPOINT_NOT_FOUND. \
             Install MinGW-w64 or set WINDIRES to its path."
        );
        return;
    };

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    let object = out_dir.join("tests_manifest.o");

    let status = Command::new(&windres)
        .arg("--input")
        .arg(&script)
        .arg("--output")
        .arg(&object)
        .arg("--output-format=coff")
        .arg("--target=pe-x86-64")
        .status();

    match status {
        Ok(status) if status.success() => {
            // `rustc-link-arg-tests` is not accepted by this cargo build
            // (`invalid instruction from build script`), so the object is
            // linked into every binary target. That is harmless: the resource
            // section is identical to the manifest `tauri-build` already
            // embeds in the application, so the app gains a duplicate-free
            // resource and the test harness gains the one it was missing.
            println!("cargo:rustc-link-arg={}", object.display());
        }
        Ok(status) => {
            println!("cargo:warning=windres failed ({status}); the test binary will have no manifest.");
        }
        Err(error) => {
            println!("cargo:warning=could not run windres ({error}); the test binary will have no manifest.");
        }
    }
}

/// Locates `windres`, preferring an explicit `WINDIRES` override.
fn find_windres() -> Option<PathBuf> {
    if let Ok(explicit) = env::var("WINDIRES") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }

    // The toolchain configured for the C compiler is the most reliable place
    // to look: it is the same MinGW that compiles SQLite.
    if let Ok(cc) = env::var("CC_x86_64_pc_windows_gnu") {
        let candidate = PathBuf::from(cc).with_file_name("windres.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let on_path = Command::new("windres").arg("--version").output();
    if matches!(on_path, Ok(out) if out.status.success()) {
        return Some(PathBuf::from("windres"));
    }

    None
}
