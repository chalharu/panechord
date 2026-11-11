# Panechord

Panechord is a declarative UI framework for Rust. You describe a window in
YAML, hold the model in a `Store`, and let generated code wire the two
together. The same declaration renders on real GTK4 widgets
(`panechord-gtk`), a Tao window with software-rendered controls
(`panechord-tao`), native AppKit (`panechord-macos`), and an opt-in WinUI 3
backend (`panechord-windows`, initial integration).

```rust
use panechord_gtk as backend;
use panechord_runtime::Store;

#[derive(Clone)]
struct Greeting {
    name: String,
}
#[panechord_macros::commands]
impl Greeting {
    #[command]
    fn greet(&mut self) {
        self.name = format!("Hello, {}!", self.name);
    }
}

fn main() -> Result<(), String> {
    backend::run_host(|host| {
        use panechord_runtime::{Session, UiHost as _};
        let session = Session::new();
        host.attach(session.clone())?;
        let executor = host.executor();
        let model = Store::new(Greeting { name: "Panechord".into() }, executor);
        let owner = model
            .try_into_owner()
            .unwrap_or_else(|_| panic!("shared model"));
        let view = panechord_macros::owned_view!("ui/minimal.yaml", owner, backend);
        #[cfg(feature = "backend-gtk")]
        host.register_view(&session, "main", &view)?;
        #[cfg(feature = "backend-tao")]
        host.present(&view);
        #[cfg(feature = "backend-macos")]
        host.present(&view);
        Ok(())
    })
}
```

```yaml
schema_version: 2
root:
  type: Window
  id: window
  title: {literal: Minimal Panechord}
  content:
    type: StackLayout
    id: layout
    padding: 16
    gap: 8
    children:
      - type: TextBox
        id: name
        text: {bind: {path: name, mode: TwoWay}}
      - type: Label
        id: greeting
        text: {bind: {path: name}}
      - type: Button
        id: greet
        text: {literal: Greet}
        command: greet
```

This is `demo/examples/minimal.rs` and `demo/ui/minimal.yaml`, minus the `cfg`
gates that switch the backend import between GTK and Tao.
Run it with `cargo run --locked -p panechord-demo --example minimal`.

Three ideas carry the whole design:

- `Store<S>` owns the model. `Binding<T>` is a typed lens into one field;
  `Command` is an argument-erased, enablement-gated action. The UI never sees
  the model struct, only bindings and commands.
- The `view!` / `owned_view!` macros generate the wiring at compile time.
  Field names, types and command signatures are checked by rustc; no `build.rs`,
  no contract crate, no runtime reinterpretation of the YAML.
- Ownership is explicit. `try_into_owner` acquires the single shutdown owner
  before a view is built; closing a shared view never closes another owner's
  store. Passing a bare `Store` to `owned_view!` is a compile error.

## Crates

| Crate | Role |
| --- | --- |
| `panechord` (root) | Public facade; re-exports the runtime and the macros |
| `runtime` | `Store`, `Binding`, `Command`, effects, shutdown coordination |
| `markup` | Strict schema-v2 YAML/JSON parser with size and shape limits |
| `macros` | `view!`, `owned_view!`, `#[commands]` code generation |
| `native` | Selects the platform-default backend |
| `gtk` | GTK4 backend |
| `tao` | Tao backend with software-rendered controls |
| `macos` | AppKit backend (builds its Swift bridge internally) |
| `windows` | Opt-in SDK-owned WinUI 3 backend; native controls and UI-local executor |
| `demo` | Control gallery, minimal example, input probe |
| `library` | Folder-scanning file list; a second worked screen |

## Requirements

- Rust 1.95 or later.
- Linux: GTK4 development files (the Tao backend also uses a GTK3 bridge).
- macOS: Xcode Command Line Tools with Swift.
- Windows: Tao (`backend-tao`) or the initial WinUI 3 backend
  (`backend-windows`). The platform default remains Tao. WinUI activation and
  app-local deployment require additional build support; see
  [Windows backend](windows/README.md).

## Running the screens

From the repository root:

```sh
cargo run --locked -p panechord-demo --example minimal
cargo run --locked -p panechord-demo
cargo run --locked -p panechord-demo --no-default-features --features backend-tao
cargo run --locked -p panechord-library -- --folder /path/to/files
```

The gallery (`demo/ui/gallery.yaml`) exercises every layout and template
against a 1000-row model. Only visible rows are mounted; editing state survives
scrolling off screen. Gallery saves go to `panechord-demo-<pid>.txt` in the
temporary directory. The input probe is
`cargo run --locked -p panechord-demo --example controls`;
set `PANECHORD_CONTROLS_REVIEW=1` for the interactive review.

WinUI 3 is selected explicitly through the facade's `windows` feature, or the
screens' `backend-windows` feature with `--no-default-features`. The screens also
offer `windows-self-contained` for cache-only activation/deployment setup.
Shared `minimal.yaml` has passed native Sandbox verification through
`panechord::backend` and `Runner`; full gallery/library native acceptance and
remaining capability gaps are separate gates, not implied by compilation.
See [Windows verification](windows/verification/README.md) for the isolated
runner, which always terminates its owned Sandbox processes after execution.

## Verification

```sh
sh scripts/verify.sh  # fmt, Clippy, core/macro suites, native backend gates
sh scripts/verify-docker.sh     # Linux formal gate on Debian/bookworm + Xvfb
sh scripts/verify-macos.sh      # AppKit/Tao gates on an interactive macOS desktop
sh scripts/measure.sh           # release CPU budgets (informational thresholds)
```

`verify.sh` succeeds with exit code 0 and `VERIFY3 PASS`. GTK tests run one
process per test under Xvfb with `GDK_BACKEND=x11`, because GTK pins thread
affinity and an inherited Wayland session would bypass Xvfb. Clipboard probes
need `PANECHORD_ALLOW_CLIPBOARD_PROBE=1` on a desktop with no concurrent
copy/paste activity. `sh scripts/check-yaml.sh check ...` maps macro
diagnostics back to YAML lines; without it, plain cargo still typechecks but
points at the `view!` call instead. Stable proc macros cannot manufacture
foreign-file spans, so this is a diagnostic helper, not an LSP.

Docker results are Linux/X11 evidence only. macOS results come from
`verify-macos.sh`; AppKit bundles are checked with
`python3 scripts/bundle-macos.py --verify`. Neither replaces real IME,
assistive-technology, or on-device Windows testing.

CI (`.github/workflows/`) runs the same gates from a prebuilt GHCR image
(`.github/workflows/docker/Dockerfile.ci`, content-hash tagged), so Linux
jobs never run `apt-get` at runtime: the single `coverage.yml` pipeline
builds one nextest archive per OS/backend set, distributes the archives to
the Ubuntu/M1-Mac/Windows run legs, executes them with
`cargo llvm-cov nextest`, runs the display examples under llvm-cov in the
examples leg, and merges the per-leg LCOV fragments on
SonarCloud. Windows/macOS archives build natively on their own runners:
GTK system libraries and the AppKit/Swift bridge do not cross-link from a
Linux sysroot.

## Documentation

- `API.md` — the frozen cross-backend contract both backends and codegen target.
- `HARDENING.md` — supported scope, rejection contract, verification procedures.
- `macros/README.md` — exact YAML syntax, generation rules, diagnostics.
- `markup/README.md` — parser limits (1 MiB, depth 64, separate 16k value-node
  and expanded-control budgets, no anchors).
- `runtime/README.md` — scheduling, effects, shutdown, scope contracts.
- `gtk/README.md`, `tao/README.md`, `macos/README.md`, `windows/README.md` — backend
  behavior and limits.
- `library/README.md` — the folder-scanning screen and its wiring pattern.

## Non-goals

- Pixel-identical rendering across backends. Native metrics, minimum sizes and
  line breaking differ; the contract is structure, reachability and state.
- Nested windows, CSS patching, editable combos, variable row heights.
- A general plugin registry: native factories are explicit `fn` paths so
  linking and typechecking stay in Rust.
- IME conversion, assistive technology, distribution signing, power-loss
  safety. Where the framework touches these (clipboard, field editors), the
  backend notes say exactly how far it goes.

## License

MPL-2.0; see `LICENSE`.
