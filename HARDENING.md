# Support Status and Verification

This document records what Panechord supports, what it explicitly does not,
and how each claim is verified. Passing these gates is not certification of
IME conversion, assistive technology, distribution signing, or hardware
failure recovery.

## Supported Scope

| Capability | GTK/Linux | Tao | AppKit |
| --- | --- | --- | --- |
| Demo gallery | Supported | Supported | Unsupported (the demo binary selects GTK/Tao only) |
| Library screen | Supported | Supported | Supported |
| Stack | Supported | Supported | Supported |
| Grid/Flex/Absolute | Supported with documented geometry limits | Supported with documented geometry limits | Unsupported |
| Items keyed retention | Supported | Supported | Supported, including editable reorder |
| Optional Content template | Supported | Supported | Supported |
| Coordinated multiwindow/attach | GtkHost | TaoHost under one owned loop | MacHost |
| Native controls | GTK | Software iced controls | AppKit |
| Full external IME/accessibility certification | Not certified | Preedit/accessibility limitations | Not certified |

One document describes one window. Window creation, ownership, reopen and shutdown
remain typed host/application composition, not a global YAML graph. TaoHost keeps
one WindowId-routed loop, per-window painter/clipboard/surface state and a shared
scheduler. Close/reopen and coordinated veto/drain are native-tested on Linux/macOS.
Its legacy external-reactor entry still rejects instead of creating a nested loop.

The schema has 25 control/layout variants. CheckBox, Radio, stepped Slider f64,
Combo Choice IDs, shared RGBA ImageFrame, retained Tab pages and nonblocking Dialog
content are declarative. Display-only TwoWay properties reject at their markup mode
location. Grid has equal-width columns, a gap, and explicit zero-based row/column
and spans; children left at the default (0,0) cell flow in declaration order
through the shared `grid_position` helper. GTK uses homogeneous, expanding native
columns; Tao divides the available width. Fixed rows are not automatically enlarged
by the window scroller. Arbitrary CSS patches, dynamic Tab-page graphs, editable
Combo and arbitrary Grid tracks remain outside the fixed supported subset.

Layout values are finite and bounded at 1,000,000 logical pixels; Grid columns
are 1..=4096. Combined margin/padding overflow rejects before native conversion.
`try_layout` rejects unavailable/foreign parents and duplicate IDs before mutation;
AppKit rejects unsupported layouts instead of returning a dangling Parent.

## Correctness Contract

| Review concern | Resolution |
| --- | --- |
| Dirty-field omissions | Handwritten `bind_scoped`/`update_scoped` are conservative. Explicit optimization APIs have separate names and documented proof obligations. Generated field readers, writers and dependency names share one parsed path. |
| Owning/shared model confusion | `owned_view` consumes a non-Clone `StoreOwner`. Acquisition fails if a shared `Store` handle already exists; compile-fail and runtime tests cover both boundaries. Sharing from an existing owner explicitly delegates final closure to it. |
| Write rejection ambiguity | `try_set` returns `ReadOnly`/`Closed`/`Capacity`/`Conversion`. Acceptance is not an application receipt. Acceptance bools are `must_use`; intentional discards are explicit. |
| Dropped continuation | Store fails closed and exposes `dispatch_failed`, rather than silently retaining a stranded FIFO. Hosts must still poll retained futures and avoid blocking callbacks. |
| Projection duplicates | `map` keeps its non-`PartialEq` contract; `distinct` is an explicit opt-in. |
| Backend Style errors | `try_style` validates spacing, finite dimensions and parent placement before mutation. Missing IDs reject, never alias root. `style` records errors; invalid GTK placement no longer panics after partial mutation. |
| Late cleanup | `on_close` invokes immediately after closure on GTK, Tao and AppKit. |
| Viewport writeback | Writable models receive effective last-page start; readonly models scroll locally. Fractional Tao overscan remains bounded at count+1. |
| Diagnostic locations | JSON also has source spans. YAML unknown fields use spanned keys. The wrapper selects primary expansion markers structurally, preserves CRLF byte positions and leaves ambiguous diagnostics unchanged. |
| Editable rows (AppKit) | Native frames move without remounting retained key/Store pairs. Native regression preserves field-editor identity, marked text and selection through reorder. |
| Distribution (AppKit) | `@rpath` bridge plus reproducible signed bundle script; verification relocates the bundle and runs smoke without DYLD overrides or absolute build rpaths. |
| Clipboard interference | Mutable clipboard probes require explicit opt-in and an isolated desktop. Synthetic input is not external IME certification. |
| Smoke readiness | Backends wait for the actual draw notification, not a fixed delay. Formal runners retain a configurable hang watchdog. |

## Common Rejection Contract

The shared controls example separates raw model data, display, acceptance and
diagnostics. Formal gates run the same probe with actual GTK, Tao and AppKit controls.

| Case | Model | Display | Result |
| --- | --- | --- | --- |
| Slider step normalization | 42.4 before input | 42.0 | Valid writes normalize before acceptance |
| Slider Closed/Capacity/Conversion | Unchanged 42.4 | Retained 42.0 | `slide` Err; `try_set` preserves typed `WriteError` |
| readonly/disabled/hidden | Unchanged | Retained, without remount | Err; `enabled=true` does not add a writer |
| unknown literal Tab page | Not constructed | Partial View closed | Parser error at selected property |
| unknown bound initial Tab page | Shared Store unchanged; consumed owner closed | Partial View closed | `try_view`/`try_owned_view` Err |
| empty Tab choice/readonly select | Empty remains empty; readonly unchanged | First page/last accepted page | readonly select Err |
| unknown Combo/Tab operation choice | Unchanged | Retained | Err before mutation |
| nested menu disabled parent | Unchanged | Retained tree | Leaf activation Err; typed argument preserved when enabled |
| Dialog readonly/hidden parent | Open flag unchanged false | No modal opened | `set_open` Err |
| Dialog open/child dismiss/reopen | Open flag true/false follows accepted operations | Native modal or Tao overlay | Commands and `set_open` accept; close then reopen verified |

Dialog open/close is an explicit model operation; a hidden placeholder is not
a user visibility request. Native modal appearance is not required to match the
Tao overlay. Operation acceptance is not a queued write's application receipt. View
error strings and native appearance are not promised byte-identical across backends.

Shutdown confirm/cancel/drain/close panic injection verifies all remaining windows
close and failures remain observable. Forced close stays idempotent. Library scan
keeps the lowest 256 paths independently of enumeration order within the 8192
visit budget; capped reports do not promise a complete folder snapshot.
Metadata-only scans preserve content and mtime.

## Deliberate Contracts

- `is_writable` reports writer capability, not current capacity/liveness. Closed
  bindings reject writes and may deliver one initial frozen value to a subscriber.
- Serial orders one slot. Latest cancellation wrappers consume bounded permits
  until polled/dropped. These are not globally serialized UI work or unbounded leaks.
- Human confirmation has no destructive automatic timeout. `cancel_shutdown`,
  force-close and drain timeout are separate operations; hooks must be observable
  and nonblocking. `u64` exhaustion remains fail-stop rather than unsafe ID reuse.
- Keys are opaque nonempty IDs excluding slash. Constructor bypass is revalidated
  at backend boundaries. Whitespace/backslash are not filesystem traversal.
- Shortcut grammar supports multiple modifiers but not all punctuation/media keys,
  chords or global conflict resolution.
- `watch` delivers initially synchronously and returns an RAII `must_use` token.
  Validation status delivery guarantees still require the same owning Store and
  positive ingress; cross-Store retry is not claimed to be independent of its owner.
- Force-close, dropped registration tokens and leaked application captures remain
  explicit lifecycle operations. Session weak registration does not guarantee
  cleanup after application-created strong cycles or `mem::forget`.

## Verification

- Core/compile-fail: runtime, markup, macros and model suites. The diagnostic
  wrapper has independent structured tests, including Unicode/CRLF/tab and ambiguity.
- Linux formal gate: `scripts/verify.sh`, separate Xvfb GTK processes, mandatory
  XTest paths, explicit ignored/native tests, each requiring an executed test.
  The GitHub workflow invokes `verify-docker.sh` on relevant pushes/PRs. Its hosted
  result must be checked after push; local success is not evidence that a remote
  workflow has already completed.
- Windows gate: `.github/workflows/panechord.yml` `windows` job builds
  `panechord-demo --no-default-features --features backend-tao`, runs strict Clippy
  on it and executes the display-free runtime/markup/macros suites. Native window
  behavior on Windows must be verified on a Windows machine; a green CI job is
  not native verification.
- macOS formal gate: `scripts/verify-macos.sh`, main-thread AppKit/Tao executables,
  library smoke, editable reorder and both relocated bundles.
- Release CPU gates: `scripts/measure.sh` and `scripts/measure-macos.sh`. Existing
  environment budget overrides remain available; watchdogs are not latency SLOs.
- Bundle build: `python3 scripts/bundle-macos.py --verify`
  (or `--package panechord-library`). Ad-hoc signing is not Developer ID signing,
  notarization or oldest-OS certification.
- Clipboard native probes require `PANECHORD_ALLOW_CLIPBOARD_PROBE=1`. Formal native
  scripts opt in; run them only on an isolated desktop with no concurrent copy/paste.
- `PANECHORD_NATIVE_TIMEOUT_SECONDS` defaults to 45 for native runners; AppKit retains
  `PANECHORD_MACOS_TIMEOUT_SECONDS`. A timeout is a failed/hung test, not a skipped success.

Report per-suite executed/failed/ignored counts. Do not equate plain `cargo test`
with completion of native, RPA, release or distribution gates. Real external IME,
Full Keyboard Access, VoiceOver, hardware power loss, Windows native behavior and
all-platform packaging remain explicit pre-release checks, not inferred successes.

## Manual Gates

The following are unverified. Passing automated gates does not mark them conformant:

1. macOS Japanese IME: kana input, candidate display, conversion, confirm/cancel,
   including during save and continuous search.
2. Focus moves during conversion, app switching, refocus and modifier-key residue
   after Cmd operations, with real keys.
3. Full Keyboard Access on/off: Tab/Shift+Tab, Space, Cmd+S/Q and unsaved-exit
   confirmation.
4. VoiceOver reachability of field names, values, readonly state, button state,
   lists, save state and errors.
5. Real input-to-display latency and compositor frames via Instruments or equivalent.
6. Windows native window/menu/clipboard behavior of the Tao demo on a Windows machine.
