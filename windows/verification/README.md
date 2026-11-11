# Windows Sandbox verification

All native applications, probes and test executables, including `build_assets`
and cleanup mocks, run **only in Windows Sandbox**. Host builds, no-run compilation
and binding generation are allowed. Use Windows 11 x64 MSVC, x64 PowerShell,
Windows Sandbox and the [exact-version caches](../README.md#self-contained-deployment).
Missing inputs fail closed; nothing downloads, installs, elevates or bypasses TLS.

## Run

From the repository root:

```powershell
& .\windows\verification\sandbox.ps1 -PrepareOnly
& .\windows\verification\sandbox.ps1 -Run -WatchdogSeconds 240
```

Default mode is preparation only: build/stage without launching Sandbox or tests.
`-Run` and `-PrepareOnly` are mutually exclusive; `-Run -KeepOpen` is rejected.
Each invocation needs a **new** artifact directory. Omit `-ArtifactDirectory` for
a unique temporary directory, or supply a new absolute path; never reuse one.

### Shared-YAML minimal probe

Build the probe separately on the host, then pass its actual Cargo artifact:

```powershell
cargo build -p panechord-demo --example windows_smoke --no-default-features --features windows-self-contained --locked --offline
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PWD 'target' }
$ProbeExe = Join-Path $TargetDir 'debug\examples\windows_smoke.exe'
& .\windows\verification\sandbox.ps1 -Run -ProbeExecutable $ProbeExe -ProbeCases @('minimal-yaml') -WatchdogSeconds 240
```

The path assumes no explicit target triple or other Cargo target-dir override.
Confirm the actual artifact; set `$ProbeExe` explicitly if it differs (default:
`target/debug/examples/windows_smoke.exe`). Do not execute it on the host.
The probe supplements, rather than replaces, `native_smoke` and required tests.
Executable and case list must be supplied together (1..24 unique safe case names).
For `minimal-yaml`, guest execution uses `--case minimal-yaml` and requires exit 0,
no timeout/capture diagnostics, stderr `PROBE minimal-yaml NATIVE_MOUNT_EFFECT`
and `PROBE minimal-yaml close-accepted`, and stdout
`NATIVE_PROBE_PASS minimal-yaml normal-exit` after the native event loop returns.

### Prebuilt artifacts

`-SkipBuild` still reads locked/offline, feature-enabled Cargo metadata. Supply
the actual `native_smoke.exe` and **every** formal package test executable:
library, test-enabled examples and integration tests, including `build_assets`.
Obtain paths from Cargo's `compiler-artifact.executable` JSON, never guessed hashes:

```powershell
cargo build --manifest-path windows/Cargo.toml -p panechord-windows --locked --offline --features self-contained,native-smoke --example native_smoke --message-format=json
cargo test --manifest-path windows/Cargo.toml -p panechord-windows --locked --offline --features self-contained,native-smoke --lib --examples --tests --no-run --message-format=json
& .\windows\verification\sandbox.ps1 -Run -SkipBuild -ApplicationExecutable $ApplicationExe -TestExecutables $TestExes -RuntimeDirectory $RuntimeDirectory -WebView2Directory $WebView2Directory
```

Set these variables to reviewed existing paths. RuntimeDirectory is the official
Runtime 2.4.0 extraction; WebView2Directory contains the 1.0.4078.44 Core DLL
(see cache paths above). Unique test paths must match all discovered test targets.
This route trusts supplied artifact provenance: it cannot prove the executables'
source or inspect their embedded manifests. Prefer the normal build route.

## Checks and verdict

- Host metadata requires registry-only `windows`/`windows-*` dependencies, no
  Winio, and reactor/setup 0.100.0. Normal builds match application/package IDs
  and every test target/artifact; missing, extra, duplicate or empty sets fail.
- Generated application/test manifests must match the template's **920** unique
  activation classes, DLL associations, threading models and COM declarations.
  Runtime allowlists/resources, pinned Microsoft signatures/hashes and relocated
  inventory sizes/SHA-256 are checked; unsafe paths/reparse points are rejected.
- Package verification requires a clean runtime environment, four distinct native
  generations/HWNDs, required lifecycle stages and app-local loaded runtime DLLs,
  including Xaml and Windowing.Core. No installed-runtime override is accepted.
- `--p1-smoke` requires exit 0 and `P1 SMOKE PASS`. Fault cases `--make-error`,
  `--task-panic`, `--reject-wake` each require exit **1**, their expected error,
  and both `FAULT_HOST_TEARDOWN_PASS` and `FAULT_TEARDOWN_PASS` markers.
- Rust harnesses require exit 0, `test result: ok.` and nonzero passed tests;
  cleanup mocks require `SANDBOX_CLEANUP_TESTS_PASS cases=7`.
- Acceptance requires harness exit **0** and **`Passed=true` in all three**:
  `output/report.json`, `output/package-verifier.json`, `output/cleanup.json`.
  Keep these files and `startup.log`; cleanup success cannot rescue a failed report.

See [backend limits](../README.md#verified-scope-and-limits) for unverified areas.

## Isolation and cleanup

Networking, vGPU and clipboard are disabled; payload mapping is read-only and
report mapping writable. Children have 30-second watchdogs; the host defaults to
240 seconds (allowed 180..240). Guest scripts guard the Sandbox account identity.
In `finally`, cleanup selects only new launch/config-linked Sandbox clients and
descendants, rechecks **PID + CreationDate**, uses `taskkill /PID <owned-pid> /T /F`
and verifies disappearance. Never use `/IM` or target preexisting/unrelated user
sessions. Remaining owned processes make cleanup fail, without elevation/retry.

## Scripts

- `sandbox.ps1`: host build/stage, watchdog and owned-process cleanup.
- `sandbox-startup.ps1`: guest bootstrap and parse/startup failure reporting.
- `sandbox-guest.ps1`: guest relocation, tests, fault cases, probes and report.
- `verify-package.ps1`: clean-environment, native identity and module-origin checks.
- `sandbox-cleanup-tests.ps1`: seven guest-only cleanup mocks using the function AST.
