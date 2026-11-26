use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=native/AppKit.swift");
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
    // Coverage instrumentation for SonarCloud Swift tracking: enabled only
    // when PANECHORD_SWIFT_COVERAGE=1. The flag pair makes the bridge emit
    // LLVM profraw entries at runtime via LLVM_PROFILE_FILE; profile data
    // is merged and exported separately (see scripts/swift-coverage.py).
    // Keep instrumented runs on a dedicated LLVM_PROFILE_FILE directory:
    // Apple's Swift emits raw profile version 10 while current Rust
    // expects version 11, so sharing one merge directory breaks the other
    // language's report. Local builds leave this unset, so release and
    // distribution dylibs keep the plain optimized bridge.
    let swift_coverage = env::var("PANECHORD_SWIFT_COVERAGE").as_deref() == Ok("1");
    let mut args: Vec<String> = vec![
        "swiftc".into(),
        "-swift-version".into(),
        "5".into(),
        "-O".into(),
        "-emit-library".into(),
        "-module-name".into(),
        "PanechordAppKit".into(),
        "-target".into(),
        format!("{arch}-apple-macosx{minimum}"),
    ];
    if swift_coverage {
        args.push("-profile-generate".into());
        args.push("-profile-coverage-mapping".into());
    }
    args.extend(
        [
            "-Xlinker",
            "-install_name",
            "-Xlinker",
            "@rpath/libPanechordAppKit.dylib",
            "native/AppKit.swift",
            "-o",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    args.push(library.to_string_lossy().into_owned());
    let result = Command::new("xcrun")
        .args(&args)
        .status()
        .expect("Xcode Command Line Tools with Swift are required");
    assert!(result.success(), "AppKit bridge compilation failed");
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=dylib=PanechordAppKit");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", output.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
}
