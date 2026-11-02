# macOS AppKit Backend

The macOS backend. It connects Swift AppKit controls to the shared runtime
and proc macros through a thin C ABI — `NSWindow` / `NSTextField` / `NSButton` /
`NSMenu`, not a reimplementation over Tao/iced.

## Running

An interactive macOS desktop, Rust 1.95 or later, and Xcode Command Line Tools
with Swift. From the repository root, pick exactly one backend:

```sh
env -u DYLD_LIBRARY_PATH -u DYLD_FALLBACK_LIBRARY_PATH -u DYLD_INSERT_LIBRARIES \
  cargo run --locked -p panechord-library --no-default-features --features backend-macos -- --folder /path/to/files
env -u DYLD_LIBRARY_PATH -u DYLD_FALLBACK_LIBRARY_PATH -u DYLD_INSERT_LIBRARIES \
  cargo run --locked -p panechord-demo --no-default-features --features backend-macos -- --smoke
sh scripts/verify-macos.sh
sh scripts/measure-macos.sh
```

Unsetting only the process's `DYLD_*` search/injection variables is required:
with `DYLD_LIBRARY_PATH=/opt/homebrew/lib` inherited, the OS ImageIO resolves
to Homebrew's PNG codec and crashes with SIGBUS during native image capture.
The global environment is left alone.

No application `build.rs` or UI contract crate is needed; only the backend's
own `build.rs` compiles Swift. Swift mode 5, stock AppKit, no extra Swift
packages. The bridge install name is `@rpath/libPanechordAppKit.dylib`.
`python3 scripts/bundle-macos.py --verify` stages the app, the bridge and
relative rpaths into a bundle and verifies ad-hoc codesigning plus a relocated
smoke run (`--package panechord-library` for the library). Copying the bare
binary out of Cargo's loader setup is not a distribution procedure. Developer
ID signing and notarization are out of scope; ad-hoc success implies nothing
about them. The default deployment target is 11.0, which certifies nothing
about the oldest OS or Intel hardware.

## Contract

- The screens are the file list and the demo gallery: Stack, Grid, Flex,
  Absolute, Label, TextBox,
  fixed-row ItemsControl, Menu, meter, plus CheckBox, Radio, Slider, Combo, Image,
  Tab, Dialog and retained visible/enabled/Style bindings, including typed
  submenus, all held to the shared contract in `API.md`.
  The demo contributes its controls probe (also runnable with `backend-macos`),
  and the `macos` examples cover conformance and multiwindow behavior.
  Grid splits the available container width into equal tracks with the declared
  gap; rows size from natural or explicit child heights, and spanned surplus
  spreads evenly across the spanned rows. Flex wraps in mount order with the
  declared gap. Absolute places children at style x/y. There is no silent
  fallback, and no claim of pixel parity with GTK/Tao.
- A main-thread `MacHost` drives the shared loop. It attaches to `UiHost` /
  `Session` and retains multiple windows. Use `run_host` with
  `register_view(session, id, view, hooks)`; shutdown ownership lives in
  explicit hooks. Window close is per-view; app Quit covers the whole Session.
  Delegates are app-wide while native menus follow the active window.
  One host holds one Session: reattaching a clone is idempotent, while
  attaching a foreign Session or registering a window against one fails before
  any side effect. `register_view` auto-attaches the first Session. A closed
  view is rejected before it can touch Session or window state.
  Closing the active window hands menus and Quit to a surviving window without
  re-`present()`. Confirmation waits without a deadline; only the post-approval
  drain is capped at five seconds. `run_host` is the single entry; screens
  attach a `Session`, build, and `present`. External reactor integration is a
  separate task.
- `DialogFooter` is a right-aligned row, Cancel first; a destructive accept sets
  `NSButton.hasDestructiveAction` (macOS 11+). A non-destructive accept is the
  default button (Return) and cancel always answers Escape; a destructive
  accept keeps no Return equivalent — the backend owns visual order, emphasis
  and key equivalents.
- TextBox delegates to the AppKit field editor: selection, undo, clipboard and
  marked IME text belong to the OS. While marked text is present, model
  notifications must not replace the `stringValue`. This is not a general edit
  transaction or a conflict-resolution guarantee against external model writes.
- A caption named `<field_id>_caption` becomes the AX label. Inputs are
  AXTextFields with stable accessibility identifiers, laid out in declaration
  order as the key-view loop; dynamic node removal updates the references.
- Cmd+S and friends convert from the shared `Shortcut` grammar to native key
  equivalents. The Edit menu plugs into the native responder chain; WM close
  and Cmd+Q route into the close request. Dynamic menus swap native items and
  handlers only after every ID and shortcut validates; a rejected update keeps
  the old menu and reports through `menu_error(id)`, which a valid update
  clears without terminating the host.
- Views own subscription tokens; callbacks capture `WeakView`. Force close is
  idempotent.
- Lists validate every key and mount only the visible rows; a visible row with
  the same key/`Store` moves instead of remounting. Conformance keeps field
  editor identity, marked text and selection across reorder. Fixed row heights,
  a 1..=256 viewport count and finite 1..4096 row heights go through the same
  checks as the shared runtime and static literals. An invalid initial viewport
  mounts nothing; an invalid update preserves the last good display, and
  recovery plus last-page behavior are tested natively. Readonly/literal
  viewports still scroll a locally accepted viewport without writing to the
  model — only TwoWay writes back. `item_keys` returns mounted keys, not the
  whole source. Row-height changes apply to existing mounts. Eviction releases
  nested list registries, viewport handlers and scoped tokens. `keep` inside a
  template belongs to that row scope, so eviction cancels validation and resets
  pending instead of accumulating subscriptions on remount; off-screen row
  changes never start validation. Scrolling still works on the last accepted
  source after an invalid snapshot.
- Control IDs and internal events are separated by ABI kind, so `__close`,
  `rows@viewport` and `__title` behave as ordinary controls and never
  misdeliver close/scroll/title operations.
- Padding and margins fold into native frame insets under a narrow contract:
  arbitrary placement, CSS compatibility and cross-backend pixel identity are
  out of scope. The layout target is 640px and wider, scrolling vertically.

## Fairness

The macOS library opts into
`Store::with_dispatch_budget(..., NonZeroUsize(64))`; the conformance example
uses a budget of 8. Plain `Store::new` / `with_limits` keep their synchronous
contract. A budgeted
store drains up to its count per turn, then yields to a weak continuation
future. FIFO order, capacity limits, watch coalescing and close-discards-queued
semantics are unchanged. An accepted write may not be applied yet when the
call returns.

The host dispatches one native event, then polls at most 32 futures once each.
New futures wait for the next turn; pending ones are not polled until woken.
With nothing ready, the native side waits up to 8ms, and worker wakes are seen
the following turn. One callback, clone, projection or poll is never split or
preempted, so there is no wall-clock or realtime guarantee here — a long poll
delays OS event handling by exactly that long.

## Verification coverage

| Area | Automated scope |
| --- | --- |
| Demo / list | Shared smoke: draft retention, readonly, save, close veto, save-wait, re-confirm, final JSON; sandbox edit bursts, Latest results, failure recovery, selection, close |
| Input | Marked text and Japanese commit injection into the field editor; real-OS clipboard copy/cut/paste; Tab / Shift+Tab |
| Menu | Native key equivalents for Primary/Cmd+S, enablement, dynamic rebuild |
| Typed menu | `MenuItem<i32>` delivered without String conversion |
| Three windows | Shared store, auxiliary reopen, menu handoff after active-window close, Quit confirm-hook reachability and denial without re-present, confirm/cancel/drain/close ordering |
| Session limits | Clone reattach, foreign-Session and closed-view registration refusal, attaching a different Session after failure |
| Validation scope | Evict/remount cycles, pending reset, no reaction to off-screen updates, no subscription growth |
| Accessibility | Role and identifier readback — not screen-reader operation |
| List mounting | 100-row model, 6 mounted rows, range moves, reorder, invalid-key refusal and recovery |
| Viewport contract | Zero mounts on invalid initial state, out-of-range count/height refused with mounts kept, valid recovery, usize::MAX last page |
| Readonly viewport | Local moves leave the model untouched while later model notifications still arrive; automation uses the same handler as native scroll |
| Menu refusal | Bad IDs, duplicate IDs, bad trailing shortcuts keep old handlers and arguments, then recover on a valid candidate |
| Nested cleanup | Repeated mount/evict of one key, registry clearing, stale-scroll invalidation, weak template-resource expiry |
| Dynamic row height | `frame_height` readback of a real native frame change with no template rebuild |
| Overload | Bulk updates and chained synthetic keys delivered before each chain completes; WeakView and close under load |
| Drawing | 640/960px native bitmaps; AppKit draw CPU time inside a chain |

Conformance runs on the main thread; never initialize AppKit from a libtest
worker. The clipboard is restored to its pre-test content, and captures go to
a dedicated temporary directory. Draw CPU time is `displayIfNeeded` cost, not
compositor presentation, and synthetic keys are not physical input. There are
no pass/fail time thresholds — assertions cover delivery-before-chain-completion,
state and mount counts. Release observations reproduce under `measure-macos.sh`
but are not API or performance guarantees. CPU regression budgets are judged
separately by `sh scripts/measure.sh`, never as native-input latency limits.
GTK/Tao native results belong to Linux and are never folded into macOS counts.

On macOS, Tao gets the same treatment on dedicated main-thread processes:
strict Clippy, real window drawing, system clipboard, gallery and demo/list
smoke. `verify-macos.sh` runs `verify-tao-macos.sh` as part of the gate.
Tao close/menu operations there are API injection, distinct from real OS
WM/accelerator delivery and external IME conformance; see
[../tao/README.md](../tao/README.md).

## Manual gates (unverified)

Passing automation implies none of the following:

1. Japanese IME on macOS: kana input, candidate window, conversion,
   confirm/cancel — including mid-save and mid-search.
2. Focus moves mid-composition, app switching, refocus, and modifier-key
   residue after Cmd operations, all with real keys.
3. Full Keyboard Access on/off: Tab/Shift+Tab, Space, Cmd+S/Q, and the
   unsaved-exit confirmation.
4. VoiceOver reachability of field names, values, readonly state, button
   state, lists, save state and errors.
5. Real input-to-display latency and compositor frames via Instruments.

macOS/AppKit and Linux/X11/GTK are the targeted OS/backend pairs, and this
macOS implementation is a limited alpha. Windows, Wayland, external IME,
VoiceOver and distributable packaging are not part of any stable release.
No API is frozen before the on-device gates above pass.
