use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=native/AppKit.swift");
    println!("cargo:rerun-if-changed=native/w3_types.h");
    println!("cargo:rerun-if-changed=native/w3_bridging.h");
    if env::var("CARGO_CFG_TARGET_OS").unwrap() != "macos" {
        return;
    }
    assert_eq!(
        env::var("HOST").unwrap(),
        env::var("TARGET").unwrap(),
        "AppKit bridge requires a native build"
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let library = output.join("libPanechordAppKit.dylib");
    let arch = if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "aarch64" {
        "arm64"
    } else {
        "x86_64"
    };
    let minimum = env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "11.0".into());
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    println!("cargo:rerun-if-env-changed=PANECHORD_SWIFT_COVERAGE");
    // Swift coverage (SonarCloud): with PANECHORD_SWIFT_COVERAGE=1 the bridge
    // emits LLVM profraw via LLVM_PROFILE_FILE (see scripts/swift-coverage.py).
    // Keep such runs on a dedicated directory: Apple's Swift emits raw profile
    // v10, current Rust expects v11. Unset keeps the plain optimized bridge.
    let swift_coverage = env::var("PANECHORD_SWIFT_COVERAGE").as_deref() == Ok("1");
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let bridging = manifest.join("native").join("w3_bridging.h");
    let mut command = Command::new("xcrun");
    command
        .args([
            "swiftc",
            "-swift-version",
            "5",
            "-O",
            "-emit-library",
            "-module-name",
            "PanechordAppKit",
            "-import-objc-header",
        ])
        .arg(&bridging)
        .args(["-target"])
        .arg(format!("{arch}-apple-macosx{minimum}"));
    if swift_coverage {
        command.args(["-profile-generate", "-profile-coverage-mapping"]);
    }
    let result = command
        .args(["-Xlinker", "-install_name", "-Xlinker"])
        .arg("@rpath/libPanechordAppKit.dylib")
        .args(["native/AppKit.swift", "-o"])
        .arg(&library)
        .status()
        .expect("Xcode Command Line Tools with Swift are required");
    assert!(result.success(), "AppKit bridge compilation failed");
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=dylib=PanechordAppKit");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", output.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
}
