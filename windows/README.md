# Windows backend

**Initial preview:** `panechord-windows` implements Panechord's Windows backend
using SDK-owned WinUI 3 Application/Window objects, a direct retained-node
renderer, a UI-local executor and an HWND-lifetime close gate. The product owns
the actual SDK Window; it does not discover/capture windows by caption or use
XAML Islands. `View::render()` is a diagnostic Reactor DSL snapshot, not the
native rendering path.

## Select and build

Use a Windows build host and Windows target. The accepted configuration is
x64 MSVC on Windows 11; other architectures/toolchains are not runtime-verified.
The crate exports `Backend` (`NAME = "windows"`), `View`, `Parent`, `WeakView`,
`Host` and `run_host`.

- Facade: enable `panechord/windows`; shared UI uses `panechord::backend` and
  `Runner::builder()`. `windows-self-contained` also enables cache-only deployment.
- Demo/library: `backend-windows` selects this backend;
  `windows-self-contained` adds executable deployment support.
- Select only one explicit backend. Demo/library still default to GTK, so use
  `--no-default-features`. The `native` Windows default remains Tao; no product
  default has switched.

The explicit facade `windows` feature selects the root `panechord::backend`
alias even alongside `native`. The crate has a crate-level Windows cfg and is
empty on other targets, consistent with the macOS crate's platform boundary;
this keeps non-Windows workspace membership compatible, not WinUI available
there. The parent's compile-only Windows CI check is configured but has **not
been executed on GitHub**; no Linux/full-CI execution is claimed here.

Initial window dimensions accept `1..=8192` logical points on each axis.
Invalid values are a no-op, preserving any pending valid size; the first window
consumes that size once. The final acceptance includes the existing
`initial_size_is_consumed_once` regression extended for invalid inputs.

From the repository root, with the existing caches described below, these are
**build-only** commands (no application/test execution):

```powershell
cargo build -p panechord-demo --no-default-features --features windows-self-contained --locked --offline
cargo build -p panechord-library --no-default-features --features windows-self-contained --locked --offline
cargo build -p panechord-demo --example windows_smoke --no-default-features --features windows-self-contained --locked --offline
```

Run native applications and test executables **only in Windows Sandbox**, through
[the verification harness](verification/README.md). It builds/stages the standalone
`native_smoke` fixture; the separately built `windows_smoke` example supplies the
optional shared-YAML `minimal-yaml` probe. The ordinary minimal application has no
automatic exit; it is not the automated probe. Do not substitute host execution.

## Self-contained deployment

Runtime 2.4.0, WebView2 1.0.4078.44 and registry reactor/setup 0.100.0 are pinned.
Under `%LOCALAPPDATA%\windows-reactor-setup\temp`, the helper requires existing,
nonempty exact-version inputs:

- `Microsoft.WindowsAppSDK.Runtime.2.4.0.nupkg` and
  `Microsoft.Web.WebView2.1.0.4078.44.nupkg`;
- `Microsoft.WindowsAppSDK.Runtime-2.4.0/MSIX/win10-<arch>/Microsoft.WindowsAppRuntime.2.msix`
  and its architecture-matching `.msix_extract` payload;
- `Microsoft.Web.WebView2-1.0.4078.44/win-<arch>/native_uap/Microsoft.Web.WebView2.Core.dll`.

Missing/invalid caches fail closed. Build support validates PE architecture,
embeds the activation manifest and copies allowlisted DLLs **and resources**
beside binaries, examples and test harnesses. It does not initialize WinUI,
download/extract/install SDKs or runtimes, elevate, or bypass TLS. Cargo
`--offline` alone would not constrain a third-party downloader; this helper does
not call setup's downloading `as_self_contained()` function. The Sandbox harness
also verifies package/signature/resource provenance and stages an existing
official x64 VC143 CRT; the build helper is not a complete package verifier.

### Another consuming executable

Library linker arguments are not transitive. In the consumer's manifest, enable
`panechord/windows-self-contained` from a `windows-self-contained` feature and
include `dep:panechord-windows` in that feature. Add an optional
`panechord-windows` build dependency under
`[target.'cfg(target_os = "windows")'.build-dependencies]`, using the matching
package version/path, `default-features = false`, and **without** its
`self-contained` feature.

In that consumer's `build.rs` `main`, call
`panechord_windows::deployment::configure_self_contained()` under
`#[cfg(all(target_os = "windows", feature = "windows-self-contained"))]`.
This hidden build-script API reads the caller's Cargo target/ABI, `OUT_DIR` and
`PROFILE`, using this crate's assets. Demo/library already do this. No manual
`RUSTFLAGS`, encoded flags or manifest linker arguments are needed. This is a
Windows-host helper, not a non-Windows-host cross-compilation facility.

## Verified scope and preview limits

The [2026-11-11 FINAL accepted receipt](verification/README.md#accepted-receipt-2026-11-11)
(`panechord-windows-product-final-0712a596285f452783d117248198f3f8`)
supersedes the earlier a820 receipt. Report, package verifier and cleanup all
passed, with harness exit 0. It records 51 Rust tests (48 library + 2 build-assets
+ 1 example), seven cleanup mocks, the normal/full P1 native fixture,
three fault teardowns and a real shared-YAML minimal TextBox event → Store →
TextBlock roundtrip in 1,148 ms with natural close. Native coverage includes layout/property
readback, retained keyed identity, callback revocation, dialogs and multiwindow
close/generation handling. Local all-targets Clippy passed for this crate, the
facade, demo and library. **Full gallery/library native execution is not established**;
the shared frontend runtime proof is the minimal probe only.

This completes the first milestone of the new-branch Windows backend work,
**not a full production release**. Release packaging and release qualification
remain separate from this Sandbox package-verifier pass.

Unsupported or unverified preview areas include Flex, Code fonts, inherited text
styles, TextBox text styles, middle ellipsis, menu shortcuts/edit selectors,
full IME/UIA behavior and physical OS keyboard/pointer input. Native property/event
tests are not physical-input or accessibility conformance tests. Lists use bounded
paging, not automatic scroll-offset virtualization; oversized snapshots are
rejected rather than silently truncated. Do not infer native behavior from the
legacy DSL snapshot's disabled-readonly/zero-size-opacity representation.

## Bindings and licenses

The normal product build uses checked-in generated SDK bindings and local
executor/close-gate modules: no historical `dev-docs` source imports, Winio,
Windows-family path/git dependencies, or upstream/P0 modifications are required.
The accepted full workspace graph contains 29 Windows-family packages, all from
the registry, and no Winio.

[Binding regeneration](bindgen/README.md) is a host-only developer operation:
official `windows-bindgen =0.100.0`, SDK 2.4.0, 27 transient WinMD inputs and
133 filters. `src/owned_bindings.rs` is 1,024,708 bytes; SHA-256:
`D879F55ECBE0B179A6943FF7C8D1C866E5A116646340EDB320713FF801949CF2`.
Do not hand-edit it. Handwritten product code follows the repository MPL-2.0
license; retain official binding/SDK terms and the Microsoft MIT notice in
`assets/LICENSE-MIT.windows-reactor-setup` for the setup-derived assets.