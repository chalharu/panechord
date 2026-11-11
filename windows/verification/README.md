# Formal Windows Sandbox verification

This directory owns the standalone verification harness for **panechord-windows**.
The four Sandbox/package scripts were copied in full from the inspected historical
windows-next harness, and sandbox-cleanup-tests.ps1 from the historical P0 harness.
Those source files are historical provenance only: nothing here resolves, imports,
dot-sources, builds, stages or runs a file from that historical source tree.
Required project inputs are the parent `../Cargo.toml` and `../assets/`; the actual
root workspace and the locked, locally cached registry packages are used.

## Files

- `sandbox.ps1`: host-only build/stage orchestration and owned Sandbox cleanup.
- `sandbox-guest.ps1`: guest-only relocation, verification, tests, fault cases,
  integrated native smoke and optional probes; publishes report and completion marker.
- `sandbox-startup.ps1`: guest-only PS5.1 parse/bootstrap failure reporting.
- `verify-package.ps1`: unchanged guest-only clean-environment, native-identity,
  dynamic-window and loaded-module-origin verifier.
- `sandbox-cleanup-tests.ps1`: unchanged seven guest-only dependency-free mocks;
  extracts ONLY the cleanup function AST, never the orchestrator's top-level code.
- `README.md`: invocation, evidence and handoff contract.

## Exact owner/parent invocation

From the repository root, in x64 PowerShell, after the parent has parsed all five
scripts and reviewed the existing exact-version caches:

```powershell
& .\windows\verification\sandbox.ps1 -Run -WatchdogSeconds 240
```

Preparation only (builds and stages, but launches neither Sandbox nor application
nor test executables):

```powershell
& .\windows\verification\sandbox.ps1 -PrepareOnly -WatchdogSeconds 240
```

Default mode is also preparation only. `-Run` and `-PrepareOnly` are mutually
exclusive. `-Run -KeepOpen` is rejected; owned Sandbox clients are always cleaned
up in `finally`. The artifact directory must be NEW; the default is a unique
`$env:TEMP\panechord-windows-sandbox-<guid>` directory. An owner may pass
`-ArtifactDirectory <new-absolute-directory>` for a deterministic report location.

### Optional shared YAML minimal probe

The parent builds the NEW shared-YAML demo example separately and sets `$ProbeExe`
to its actual x64 executable path. This harness does not build or change that demo.
The additional probe is NOT a replacement for native_smoke or any required tests:

```powershell
& .\windows\verification\sandbox.ps1 -Run -WatchdogSeconds 240 -ProbeExecutable $ProbeExe -ProbeCases @('minimal-yaml')
```

The probe is staged as `native-family.exe`, and guest invocation is exactly
`native-family.exe --case minimal-yaml`. Other existing native probe cases retain
legacy positional arguments. The case-name safety rule and limit of 1..24 unique
cases are unchanged; an executable and nonempty case list must be given together.

**Exact unchanged guest read/decision contract for the parent demo implementation:**

- Stderr contains `PROBE minimal-yaml NATIVE_MOUNT_EFFECT`.
- Stderr contains `PROBE minimal-yaml close-accepted`.
- Stdout contains `NATIVE_PROBE_PASS minimal-yaml normal-exit`.
- Exit code is 0, no timeout, and zero child/capture diagnostics.

These are the original guest's literal, case-sensitive `Contains` decisions,
substituting the case name; no new marker or relaxed predicate was introduced.
Emit mount/readback and accepted-close evidence only after actual native backend
operations, and the normal-exit marker only after the event loop returns. Merely
parsing YAML or printing markers is not native verification. Each probe result is
recorded in `report.Probes` with the actual arguments/stdout/stderr, and any failed
requested probe makes the overall report fail.

### Explicit prebuilt staging

`-SkipBuild` still reads feature-enabled offline Cargo metadata. `$ApplicationExe`
must be the NEW formal package's `native_smoke.exe`; `$TestExes` must contain every
actual formal package library, test-enabled example and integration-test harness,
including `build_assets`, obtained from the no-run Cargo JSON below. Provide the
existing official extracted Runtime 2.4.0 and WebView2 directories explicitly when
they are not alongside the application. The parent supplies actual paths, not
invented hashed test names:

```powershell
& .\windows\verification\sandbox.ps1 -Run -SkipBuild -WatchdogSeconds 240 -ApplicationExecutable $ApplicationExe -TestExecutables $TestExes -RuntimeDirectory $RuntimeDirectory -WebView2Directory $WebView2Directory
```

Append `-ProbeExecutable $ProbeExe -ProbeCases @('minimal-yaml')` to include the
parent-built demo probe. Unique test path count must equal the discovered formal
test-target count. This explicit-input route trusts the parent's Cargo-artifact
provenance; it does not prove a supplied PE was compiled from the formal sources
or inspect its embedded manifest. The normal build route checks package IDs,
all target artifacts and the generated application/test manifests automatically.
Do not use historical fixture binaries even though a compatibility name is retained.

## Host build/metadata contract (no host test execution)

The orchestrator internally invokes these exact Cargo argument sequences with
`<manifest>` resolved to this directory's parent `Cargo.toml`:

```text
cargo metadata --manifest-path <manifest> --locked --offline --features self-contained,native-smoke --format-version=1
cargo build --manifest-path <manifest> -p panechord-windows --locked --offline --features self-contained,native-smoke --example native_smoke --message-format=json
cargo test --manifest-path <manifest> -p panechord-windows --locked --offline --features self-contained,native-smoke --lib --examples --tests --no-run --message-format=json
```

- Metadata uses the real root workspace graph and activates optional setup. No
  historical fixed count of Windows packages is assumed; multiple registry
  package versions elsewhere in the workspace are valid.
- Every package named `windows` or starting `windows-` must have exactly the
  published crates.io registry source, never path/git/patch. Winio packages are
  rejected. Reactor/setup must each be the locked published 0.100.0 package.
- Application artifact must match the actual `panechord-windows` package ID,
  `native_smoke` name, `example` kind and non-test profile. The package must be a
  root-workspace member with the actual parent manifest path.
- All test-profile executable artifacts must match that SAME package ID. Every
  metadata test-enabled lib/example/integration target must have exactly one
  artifact; required kinds and `build_assets` are checked. Missing, duplicate,
  extra and empty test sets fail closed. Other workspace packages are not tested.
- Formal build.rs already embeds the manifest across examples and harnesses;
  no old bins-only setup call or extra global Rust flags are injected.
- Generated application and test manifests must exactly match the formal asset
  template expanded with 920 unique activation classes, including class-to-DLL
  association and `threadingModel="both"`. Only XML comments, CRLF/LF and outer
  whitespace are ignored; the description and COM registrations are retained.
  This avoids comparing the full official registry manifest with a placeholder.
- The formal runtime allowlist must match the pinned official registry allowlist
  (case-normalized, order-independent). Existing critical-file/resource checks
  and all three pinned Microsoft SHA256/signature checks are unchanged.
- Exact cached NuGet packages, MSIX and extraction/WebView2 input are preflighted
  before building. No download/install helper is invoked. A missing cache fails
  closed. Only the existing official VS2022 Professional x64 VC143 CRT is staged;
  unavailable CRT is reported, never installed.

**Compatibility filenames are not fixture binaries:** the NEW formal
`native_smoke.exe` is copied to payload `panechord-windows-next.exe` solely to keep
unchanged verifier/guest filename checks. `C:\p0-input`, `C:\p0-output`,
`C:\next-package` and optional `native-family.exe` are likewise compatibility
staging names. `configuration.Host` records actual application/test source paths,
package ID and formal target names. Original `SetupManifestSHA256` provenance is
retained; formal template/activation SHA256 fields are additional provenance.

## Preserved verdict, isolation and cleanup

- All application, package-verifier, native probes, Rust harnesses (even pure
  build_assets tests) and seven cleanup mocks run ONLY inside Sandbox. Guest,
  startup, verifier and mock scripts guard the WDAGUtilityAccount identity.
- WSB networking, vGPU and clipboard are disabled. Payload mapping is read-only;
  report mapping is writable. Reparse points, escaping inventory paths, unsafe
  leaves and executable runtime resources are rejected; relocation checks every
  inventoried file's size and SHA256.
- Runtime 2.4.0 / WebView2 1.0.4078.44 / reactor+setup 0.100.0 remain pinned.
  Installed non-CBS Windows App Runtime frameworks fail the clean-environment
  verdict; no install/removal, elevation, TLS bypass or SDK download is attempted.
- Each guest child has the unchanged 30-second watchdog. The host defaults to
  240 seconds (original permitted range 180..240), then cleans up in `finally`.
- Three fault cases require exit 1 and their original message PLUS stdout
  `FAULT_HOST_TEARDOWN_PASS` and `FAULT_TEARDOWN_PASS`: `--make-error`,
  `--task-panic`, `--reject-wake`. Integrated `--p1-smoke` requires exit 0 and
  `P1 SMOKE PASS`; verifier invokes the same full fixture via `--smoke`.
- The unchanged package verifier requires four distinct nonzero native
  generations/HWNDs (main twice, sibling once, auxiliary once), all seven original
  `STAGE` messages, and only app-local loaded runtime DLLs, including Xaml and
  Windowing.Core. Two initial READY callbacks are required before module inventory.
- Rust harnesses require exit 0, `test result: ok.` and at least one passed test;
  no empty-success verdict. Seven cleanup mocks require the exact
  `SANDBOX_CLEANUP_TESTS_PASS cases=7` marker.
- `Stop-OwnedSandbox` is text-unchanged: identify NEW session/config-linked clients,
  discover only Sandbox descendants, recheck PID+CreationDate, taskkill only owned
  PIDs, recheck disappearance and write `cleanup.json`. Existing/unrelated clients
  are never targeted; rejected termination fails closed without elevation/retry.
- Original mocks: owned-and-existing, handoff, empty, preexisting-config, pid-reuse,
  exit-race, reject. Their source/decisions are unchanged.
- Guest atomically publishes `output/report.json` before `output/complete.marker`.
  Startup failures also publish failure/marker. Host accepts only `Passed=true`,
  and a cleanup failure still makes the host invocation fail. Retain report,
  package-verifier.json, cleanup.json and startup.log as the parent's run evidence.

## Accepted receipt (2026-11-11)

The latest **FINAL** accepted artifact is
`%TEMP%\panechord-windows-product-final-0712a596285f452783d117248198f3f8`.
Direct reads of `output/report.json`, `output/package-verifier.json` and
`output/cleanup.json` confirm **all three `Passed=true`**; the sibling
`.acceptance-receipt.json` and `.final-evidence.json` confirm harness exit 0
and matching final evidence. Guest execution ran from
`2026-11-11T01:44:23.5691939Z` to `2026-11-11T01:45:06.6796263Z`
(43,110.4324 ms); the host harness took 144,857 ms.
These are local acceptance artifacts, not files distributed with the crate.

This is the first completed milestone of the new-branch Windows backend work,
not a full production release. The accepted sources already include invalid
initial-size no-op handling (`1..=8192` on both axes, preserving a pending valid
size) and the crate-level Windows cfg (empty on other targets, like the macOS
crate, for non-Windows workspace compatibility). The existing
`initial_size_is_consumed_once` regression passed with invalid-input coverage.
The explicit facade `windows` feature selects `panechord::backend`; the Windows
`native` default remains Tao. No source changes are implied by this docs update.

The actual Cargo target was
`D:\.cargo\target\panechord-windows-product-verified`, confirmed in
`report.Configuration.Host`; it was not the older damaged target. Builds used
`CARGO_INCREMENTAL=0`. With `CARGO_TARGET_DIR` and `CARGO_INCREMENTAL` scoped to
the run and restored in `finally`, the accepted invocation from the repository
root was:

```powershell
& .\windows\verification\sandbox.ps1 -Run -ArtifactDirectory "$env:TEMP\panechord-windows-product-final-0712a596285f452783d117248198f3f8" -ProbeExecutable 'D:\.cargo\target\panechord-windows-product-verified\debug\examples\windows_smoke.exe' -ProbeCases @('minimal-yaml') -WatchdogSeconds 240
```

For a rerun, use a **new** artifact directory and a freshly built probe's actual
Cargo artifact path; do not overwrite or reuse this receipt directory. Host
builds, metadata, Clippy, no-run compilation and binding generation are allowed;
all application/test execution in this receipt occurred only inside Sandbox.

| Check | Actual result |
| --- | --- |
| Rust harnesses | 51 passed: 48 library + 2 `build_assets` + 1 example; no failures/ignored tests |
| Cleanup mocks | Seven passed, exit 0 |
| Normal package verifier | Exit 0; full native fixture, four distinct generations/HWNDs and all seven `STAGE` markers |
| Four cases | `p1-smoke` exit 0; `make-error`, `task-panic`, `reject-wake` expected exit 1, each with both teardown markers |
| Shared YAML probe | `minimal-yaml` exit 0 in 1,148 ms, no timeout/diagnostics |
| Isolation | Frameworks before/after 0/0; nine observed runtime modules all app-local under relocated `C:\next-package`; installed-runtime override false |
| Dependency graph | Full 11-member workspace; 29 Windows-family registry packages; no Windows path/git source or Winio |

The probe observed generation 1, HWND `0x120042`, Loaded/shared XamlRoot,
TextBox token 3, TextBlock token 6 and one actual TextChanged event. Native
`TextBox.SetText` propagated `"Native shared YAML roundtrip"` through the event
to Store and TextBlock. Approved close, NCDESTROY, owned Store closure and natural
Application.Start return preceded `NATIVE_PROBE_PASS minimal-yaml normal-exit`.
This is not a physical-keyboard claim. Full native P1 is separately covered by
the fixture; the full demo gallery/library application was **not** executed.

Cleanup identified PID 19376 by creation time
`2026-11-11T10:43:45.192702+09:00`; owned child PID 32924 was terminated by
`taskkill /PID 19376 /T /F` (exit 0). The record has `Gone=true`,
`RemainingOwned=[]` and `Passed=true`; unrelated/preexisting sessions were not
targets. The final evidence records no live Sandbox processes. All four recorded
launched batches (PIDs 34872, 29008, 6836 and 19376, including this FINAL run)
have taskkill exit 0, `Gone=true`, `RemainingOwned=[]` and cleanup PASS. Cleanup
success does **not** promote a failed guest report to acceptance. These PIDs are
historical evidence, not cleanup instructions.

Source/staged/execution-artifact hashes match (each SHA-256 is 64 hex digits):

- `native_smoke.exe` / `panechord-windows-next.exe`:
  `40C38E959EE328B82CE62374154078F7C5BB36E3D2A34F5E3456AD972C91FFBA`.
- `windows_smoke.exe` / `native-family.exe`:
  `DAA2F9D1C017923496F2569BA69EABDBFF407BE11BF481E831D305E9E5DB7F00`.

The sibling `.acceptance-receipt.json` and `.final-evidence.json` record exit 0
for workspace fmt checking and local all-targets Clippy (`-D warnings`,
locked/offline) for Windows
`self-contained,native-smoke` and facade/demo/library `windows-self-contained`
(the latter three with `--no-default-features`), plus the demo probe build.
Formatting changed no source files. All five PowerShell scripts parsed without
errors. Final environment and working-directory restoration are true with no
environment differences, measured after restoration; this supersedes earlier
wrapper records that compared before restoration and reported false. The parent
added compile-only opt-in WinUI checks to the Windows CI job; those configured
checks have **not run on GitHub**. Local four-group Clippy success is not a
GitHub CI result, and no Linux or full-CI execution is claimed. No host
application/tests, SDK/runtime install/download, elevation, TLS bypass, upstream
edits or commits were part of the accepted run.

Full IME/UIA behavior, physical keyboard/pointer input, the full demo gallery and
library application, and release packaging/qualification remain separate,
unverified work. The Sandbox package-verifier PASS is not a production-release
or accessibility-conformance verdict. License provenance for setup-derived
assets is retained in `../assets/LICENSE-MIT.windows-reactor-setup`.

### Earlier accepted receipt (superseded)

`%TEMP%\panechord-windows-product-root-fixed-a820d96358864587b36db101a7fe4107`
was an earlier accepted run, not the latest source acceptance. The FINAL receipt
above supersedes its executable hashes, probe timing and runtime identities.

### Earlier failures are not acceptance

An earlier batch actually had `report.Passed=false`: the shared-YAML probe failed
with `unknown node: __root`. It is not a PASS despite an earlier wrapper's
unreliable exit-0 summary. A separate earlier batch also used the wrong target.
Those outcomes are historical failures, not evidence for the receipt above.
An older target produced compiler ICE/stale-decode error 244; execution stopped
there. The fresh target with incremental compilation disabled resolved the
observed build failure, but does **not** prove the underlying corruption's root
cause. The accepted receipt, rather than a reconstruction of every failed trace,
is the current runtime evidence.

## Harness preservation evidence

Read-only file comparison, normalizing only CRLF/LF and final newlines, confirmed:

- startup, package verifier and seven-mock script match their inspected originals;
- `Stop-OwnedSandbox` matches its original function text;
- the host `if ($Run)` watchdog through final cleanup/exit matches the original;
- guest's only operational change is selecting `--case minimal-yaml` arguments;
  the success predicate and all other original decisions are unchanged.

These comparisons describe preservation of the harness, not independent runtime
proof. The accepted run above supplies the current execution evidence; historical
P0/upstream sources remain untouched.
