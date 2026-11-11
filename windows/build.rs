//! Cache-only adaptation of the published windows-reactor-setup 0.100.0 assets.
//! Its public as_self_contained() returns (), can download/extract packages, and
//! emits rustc-link-arg-bins only. Do not call it here: examples and test harnesses
//! need the same official registrations, and missing caches must fail closed.
//! Link arguments apply to the calling package; downstream apps must call the
//! shared deployment helper from their own build scripts.
use std::{
    collections::BTreeSet,
    env, fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const RUNTIME_VERSION: &str = "2.4.0";
const WEBVIEW2_VERSION: &str = "1.0.4078.44";
const RUNTIME_FILES: &str = include_str!("assets/runtime.txt");
const APP_MANIFEST: &str = include_str!("assets/app.manifest");
const ACTIVATION: &str = include_str!("assets/activation.txt");
const CORE_DLL: &str = "Microsoft.Web.WebView2.Core.dll";
const MANIFEST_SLOT: &str = "<!-- ACTIVATION_CLASSES -->";

fn main() {
    for path in [
        "build.rs",
        "assets/runtime.txt",
        "assets/app.manifest",
        "assets/activation.txt",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    for variable in [
        "CARGO_FEATURE_SELF_CONTAINED",
        "LOCALAPPDATA",
        "XDG_CACHE_HOME",
        "HOME",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var_os("CARGO_FEATURE_SELF_CONTAINED").is_none()
    {
        return;
    }
    configure_self_contained();
}

/// Configure the calling Cargo package's self-contained Windows deployment.
///
/// Call from a Windows consumer's build script when its self-contained feature
/// is enabled. Reads the caller's Cargo target, OUT_DIR and PROFILE at invocation
/// time, not this library's compiled feature flags. Emits caller-local manifest
/// link arguments and stages app-local payloads from exact existing caches.
/// No native initialization, download, extraction or installation is performed.
/// Missing inputs or mismatched cached PE architectures panic (fail closed).
pub fn configure_self_contained() {
    assert_eq!(
        env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("windows"),
        "self-contained deployment requires a Windows Cargo target"
    );
    for path in [
        "build.rs",
        "assets/runtime.txt",
        "assets/app.manifest",
        "assets/activation.txt",
    ] {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        println!("cargo:rerun-if-changed={}", source.display());
    }
    for variable in ["LOCALAPPDATA", "XDG_CACHE_HOME", "HOME"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let target_env = env::var("CARGO_CFG_TARGET_ENV").expect("CARGO_CFG_TARGET_ENV");
    let target_abi = env::var("CARGO_CFG_TARGET_ABI").unwrap_or_default();
    let prefix = match (target_env.as_str(), target_abi.as_str()) {
        ("msvc", _) => "",
        ("gnu", "llvm") => "-Wl,",
        _ => panic!("self-contained WinUI requires MSVC or GNU LLVM, got {target_env}{target_abi}"),
    };
    let (arch, machine) = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => ("x64", 0x8664),
        Ok("x86") => ("x86", 0x014c),
        Ok("aarch64") => ("arm64", 0xaa64),
        other => panic!("unsupported self-contained Windows architecture: {other:?}"),
    };
    let cache = cache_dir();
    let runtime = cache.join(format!("Microsoft.WindowsAppSDK.Runtime-{RUNTIME_VERSION}"));
    let webview = cache.join(format!("Microsoft.Web.WebView2-{WEBVIEW2_VERSION}"));
    // Require the exact cached packages as well as their extracted payloads.
    // There is intentionally no downloader, tar invocation, install or fallback.
    for package in [
        format!("Microsoft.WindowsAppSDK.Runtime.{RUNTIME_VERSION}.nupkg"),
        format!("Microsoft.Web.WebView2.{WEBVIEW2_VERSION}.nupkg"),
    ] {
        require_file(&cache.join(package));
    }
    require_file(&runtime.join(format!(
        "MSIX/win10-{arch}/Microsoft.WindowsAppRuntime.2.msix"
    )));
    let extract = runtime.join(".msix_extract");
    let core = webview.join(format!("win-{arch}/native_uap/{CORE_DLL}"));
    require_file(&core);
    check_machine(&core, machine);
    for required in [
        "Microsoft.UI.Xaml.dll",
        "Microsoft.UI.Xaml.Controls.dll",
        "Microsoft.UI.Windowing.Core.dll",
        "Microsoft.UI.Windowing.dll",
        "Microsoft.UI.Input.dll",
        "Microsoft.WindowsAppRuntime.dll",
        "CoreMessagingXP.dll",
        "dcompi.dll",
        "resources.pri",
    ] {
        require_file(&extract.join(required));
    }
    assert!(
        extract.is_dir(),
        "missing cached MSIX extraction: {} (no download attempted)",
        extract.display()
    );
    println!("cargo:rerun-if-changed={}", extract.display());
    println!("cargo:rerun-if-changed={}", core.display());
    let allowed: BTreeSet<_> = RUNTIME_FILES
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    let mut payload = Vec::new();
    for entry in fs::read_dir(&extract).expect("read cached MSIX extraction") {
        let entry = entry.expect("read cached MSIX entry");
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if allowed.contains(name.to_ascii_lowercase().as_str()) {
            validate_tree(&entry.path(), machine);
            payload.push(entry.path());
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let profile = env::var_os("PROFILE").expect("PROFILE");
    let destination = out
        .ancestors()
        .find(|path| path.file_name() == Some(profile.as_os_str()))
        .expect("OUT_DIR must be inside Cargo's target profile directory");
    // Native examples live under examples/, harnesses under deps/, and ordinary
    // package binaries at the profile root (also with an explicit --target).
    for destination in [
        destination.to_path_buf(),
        destination.join("examples"),
        destination.join("deps"),
    ] {
        fs::create_dir_all(&destination).expect("create app-local payload directory");
        for source in &payload {
            copy_tree(
                source,
                &destination.join(source.file_name().expect("payload name")),
            );
        }
        fs::copy(&core, destination.join(CORE_DLL)).expect("stage cached WebView2 projection");
    }
    let manifest = out.join("app.manifest");
    let xml = activation_manifest();
    fs::write(&manifest, xml).expect("write official self-contained activation manifest");
    // Unlike rustc-link-arg-bins this covers examples, tests and benches too.
    // Cargo does not propagate a library build script's link args to consumers;
    // consuming applications must configure their own manifest and deployment.
    println!("cargo:rustc-link-arg={prefix}/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg={prefix}/MANIFESTINPUT:{}",
        manifest.display()
    );
}

fn cache_dir() -> PathBuf {
    let base = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CACHE_HOME").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .expect("existing setup cache requires LOCALAPPDATA, XDG_CACHE_HOME or HOME");
    base.join("windows-reactor-setup/temp")
}

fn require_file(path: &Path) {
    assert!(
        fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() != 0),
        "missing or empty cached self-contained input: {} (no download/install attempted)",
        path.display()
    );
}

fn check_machine(path: &Path, machine: u16) {
    // setup 0.100.0 uses a shared .msix_extract directory. Reject a cached
    // extraction for another architecture rather than silently staging it.
    let mut file = fs::File::open(path).expect("open cached PE image");
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .expect("read cached PE DOS header");
    assert_eq!(&dos[..2], b"MZ", "not a PE image: {}", path.display());
    let offset = u32::from_le_bytes(dos[60..64].try_into().expect("PE offset"));
    file.seek(SeekFrom::Start(u64::from(offset)))
        .expect("seek PE header");
    let mut header = [0u8; 6];
    file.read_exact(&mut header)
        .expect("read PE signature and machine");
    assert_eq!(
        &header[..4],
        b"PE\0\0",
        "invalid PE signature: {}",
        path.display()
    );
    assert_eq!(
        u16::from_le_bytes([header[4], header[5]]),
        machine,
        "cached DLL architecture differs from target: {} (no extraction/download attempted)",
        path.display()
    );
}

fn validate_tree(path: &Path, machine: u16) {
    let metadata = fs::symlink_metadata(path).expect("inspect cached payload");
    assert!(
        !metadata.file_type().is_symlink(),
        "cached payload may not be a symlink: {}",
        path.display()
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(path).expect("read cached resource directory") {
            validate_tree(&entry.expect("read cached resource entry").path(), machine);
        }
    } else {
        require_file(path);
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            check_machine(path, machine);
        }
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    if source.is_dir() {
        fs::create_dir_all(destination).expect("create app-local resource directory");
        for entry in fs::read_dir(source).expect("read cached resources") {
            let entry = entry.expect("read cached resource");
            copy_tree(&entry.path(), &destination.join(entry.file_name()));
        }
    } else {
        fs::copy(source, destination)
            .unwrap_or_else(|error| panic!("stage {}: {error}", source.display()));
    }
}

fn activation_manifest() -> String {
    let mut xml = String::new();
    let mut file_open = false;
    let mut namespace = "";
    let mut seen = BTreeSet::new();
    for line in ACTIVATION
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if let Some(file) = line.strip_prefix('@') {
            assert!(
                file.bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_')),
                "invalid DLL asset name"
            );
            if file_open {
                xml.push_str("    </asmv3:file>\n");
            }
            xml.push_str(&format!("    <asmv3:file name=\"{file}\">\n"));
            file_open = true;
            namespace = "";
        } else if let Some(prefix) = line.strip_suffix(':') {
            assert!(
                prefix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.'),
                "invalid namespace asset"
            );
            namespace = prefix;
        } else {
            assert!(
                file_open && !namespace.is_empty(),
                "class without official DLL/namespace"
            );
            for class in line.split_whitespace() {
                assert!(
                    class
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'),
                    "invalid class asset"
                );
                let name = format!("{namespace}.{class}");
                assert!(
                    seen.insert(name.clone()),
                    "duplicate official registration: {name}"
                );
                xml.push_str(&format!("        <winrtv1:activatableClass name=\"{name}\" threadingModel=\"both\"></winrtv1:activatableClass>\n"));
            }
        }
    }
    assert!(file_open && !seen.is_empty(), "empty activation asset");
    xml.push_str("    </asmv3:file>\n");
    assert_eq!(
        APP_MANIFEST.matches(MANIFEST_SLOT).count(),
        1,
        "manifest expansion slot"
    );
    APP_MANIFEST.replace(MANIFEST_SLOT, &xml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_manifest_expansion_is_complete_and_has_no_custom_registration() {
        let xml = activation_manifest();
        assert_eq!(xml.matches("<winrtv1:activatableClass ").count(), 920);
        assert_eq!(xml.matches("threadingModel=\"both\"").count(), 920);
        assert_eq!(xml.matches("<asmv3:comClass ").count(), 1);
        assert_eq!(xml.matches("<asmv3:comInterfaceProxyStub ").count(), 3);
        assert!(!xml.contains(MANIFEST_SLOT));
        assert!(xml.contains("<description>windows-reactor-self-contained</description>"));
        assert!(xml.contains("name=\"Microsoft.UI.Xaml.Application\""));
        assert!(xml.contains("name=\"Microsoft.UI.Xaml.Window\""));
        assert!(xml.contains("name=\"Microsoft.UI.Xaml.Controls.XamlControlsResources\""));
        assert!(
            xml.contains(
                "name=\"Microsoft.UI.Xaml.XamlTypeInfo.XamlControlsXamlMetaDataProvider\""
            )
        );
        assert_eq!(RUNTIME_VERSION, "2.4.0");
        assert_eq!(WEBVIEW2_VERSION, "1.0.4078.44");
    }

    #[test]
    fn runtime_allowlist_is_unique_and_includes_native_host_dependencies() {
        let files: Vec<_> = RUNTIME_FILES
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        let unique: BTreeSet<_> = files.iter().copied().collect();
        assert_eq!(files.len(), unique.len());
        for name in [
            "microsoft.ui.xaml.dll",
            "microsoft.ui.xaml.controls.dll",
            "microsoft.ui.windowing.core.dll",
            "microsoft.ui.windowing.dll",
            "microsoft.ui.input.dll",
            "coremessagingxp.dll",
            "dcompi.dll",
            "resources.pri",
        ] {
            assert!(unique.contains(name));
        }
    }
}
