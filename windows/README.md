# Windows backend

**Initial preview:** `panechord-windows` owns SDK WinUI 3 Application/Window
objects, a retained-node renderer, a UI-local executor and an HWND-lifetime close
gate. It does not use XAML Islands. `View::render()` is a diagnostic DSL snapshot,
not the native rendering path. Runtime verification covers Windows 11 x64 MSVC;
other architectures/toolchains are unverified.

## Select and build

- Facade: `panechord/windows` selects `panechord::backend`, used with
  `Runner::builder()`. `windows-self-contained` adds cache-only deployment.
- Demo/library: select `backend-windows` or `windows-self-contained` with
  `--no-default-features` (their default is GTK).
- Choose only one explicit backend. An explicit Windows feature overrides
  `native`; the Windows `native` default remains Tao.
- Backend features are `self-contained` (deployment) and `native-smoke`
  (verification fixture). The crate is empty on non-Windows targets.

From the repository root on Windows, with the caches below, build only:

```powershell
cargo build -p panechord-demo --no-default-features --features windows-self-contained --locked --offline
cargo build -p panechord-library --no-default-features --features windows-self-contained --locked --offline
```

Run all native applications and test executables **only in Windows Sandbox** via
[verification](verification/README.md), including the shared-YAML probe documented
there. Host compilation, build scripts and binding generation are allowed.

## Self-contained deployment

Runtime **2.4.0**, WebView2 **1.0.4078.44** and registry reactor/setup **0.100.0**
are pinned. Under `%LOCALAPPDATA%\windows-reactor-setup\temp`, require these
existing, nonempty inputs (paths below are relative to that cache):

- `Microsoft.WindowsAppSDK.Runtime.2.4.0.nupkg`
- `Microsoft.Web.WebView2.1.0.4078.44.nupkg`
- `Microsoft.WindowsAppSDK.Runtime-2.4.0\MSIX\win10-x64\Microsoft.WindowsAppRuntime.2.msix`
- `Microsoft.WindowsAppSDK.Runtime-2.4.0\.msix_extract\`: architecture-matching
  DLLs and resources, including nonempty `Microsoft.UI.Xaml` and `en-us` directories.
- `Microsoft.Web.WebView2-1.0.4078.44\win-x64\native_uap\Microsoft.Web.WebView2.Core.dll`

Missing/invalid caches fail closed: no download, extraction, installation,
elevation or TLS bypass. The helper validates PE architecture, embeds the
activation manifest and stages allowlisted DLLs/resources beside binaries,
examples and test harnesses. The Sandbox harness adds package/signature checks
and stages an existing official VS2022 x64 VC143 CRT; it never installs one.

### Consuming executables

Linker arguments are not transitive: each executable package needs a `build.rs`.
Its `windows-self-contained` feature should enable `panechord/windows-self-contained`
and `dep:panechord-windows`. Add an optional `panechord-windows` build dependency
under `[target.'cfg(target_os = "windows")'.build-dependencies]`, using the matching
version/path and `default-features = false`, **without** its `self-contained` feature.
In `build.rs`'s `main`, call the hidden build-script API
`panechord_windows::deployment::configure_self_contained()` under
`#[cfg(all(target_os = "windows", feature = "windows-self-contained"))]`.
See [demo/build.rs](../demo/build.rs) and [demo/Cargo.toml](../demo/Cargo.toml)
for the exact sample. No manual linker flags are needed; a Windows host is required.

## Verified scope and limits

Sandbox verification established a shared `minimal.yaml` native TextBox event
→ Store → TextBlock roundtrip and natural close through the facade and `Runner`.
Native fixtures also cover layout/property readback, keyed identity, callback
revocation, dialogs and multiwindow close/generation handling. Full gallery/library
execution, IME/UIA, physical keyboard/pointer input and release qualification remain
unverified; this is not production or accessibility-conformance acceptance.

Unsupported preview areas include Flex, Code fonts, inherited/TextBox text styles,
middle ellipsis and menu shortcuts/edit selectors. Lists use bounded paging, not
scroll-offset virtualization; oversized snapshots are rejected.

## Bindings and licenses

Product builds use checked-in [generated bindings](bindgen/README.md); do not
hand-edit them. Handwritten code is MPL-2.0; retain official `MIT OR Apache-2.0`
binding notices, Microsoft SDK terms and the setup-derived assets' Microsoft MIT
notice at `assets/LICENSE-MIT.windows-reactor-setup`.