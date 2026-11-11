# SDK binding generation

This is a standalone, unpublished Cargo workspace with its own lockfile.
Only the official registry `windows-bindgen =0.100.0` generates
`../src/owned_bindings.rs`. Do not patch or copy that generated file.

## Provenance and licenses

- Read-only metadata source: the Microsoft `windows-rs` repository at revision
  `a57cc321bfdbac6bb98c488ba49f399da8fcb98a`.
- Source subtree: `crates/tools/reactor/winmd`; exactly 27 WinMD blobs.
- SDK version checked against that revision's tool source: `2.4.0`.
- Exactly 133 filters, plus the official generator's default Windows metadata.
- `inputs/PROVENANCE.txt` records the revision and Git **blob object IDs**
  (SHA-1, not SHA-256). Metadata is extracted with read-only `git show`.
- The 27 `inputs/*.winmd` files are transient, locally regenerated inputs and
  are ignored by this tool's `.gitignore`, along with its local `target/`.
  Keep `inputs/PROVENANCE.txt`, the lockfile and generated Rust in version control;
  normal product builds do not need the metadata files or the upstream clone.
- The official windows-rs tool and generated bindings are distributed under
  `MIT OR Apache-2.0`; retain upstream license notices. The WinMD files remain
  SDK metadata subject to their upstream Microsoft SDK terms; this tool does
  not relicense the SDK or runtime payload. The product's handwritten sources
  have the repository's MPL-2.0 license. Setup-derived product assets retain the
  Microsoft MIT notice at `../assets/LICENSE-MIT.windows-reactor-setup`.
- No handwritten GUID/vtable, private Rust binding copy, upstream patch, Winio,
  SDK/runtime installation, or network download is involved in extraction.

## Regenerate on the host

Run `cargo run --locked --manifest-path windows/bindgen/Cargo.toml -- D:\Repositories\github.com\microsoft\windows-rs`
from the product root, with `CARGO_TARGET_DIR` temporarily set to
`D:\.cargo\target\panechord-windows-product-bindgen`; restore the previous
environment afterward. The first run may retrieve published Cargo dependencies
using normal TLS. A local clone containing the pinned revision is required;
the tool never fetches or modifies it.

The generator writes the metadata inputs, provenance, and generated Rust.
Unexpected additional WinMD inputs are rejected. Hash the output with SHA-256
and regenerate a second time to verify reproducibility. This executes only the
host generator, never a native application or test executable.

Verified output: 1,024,708 bytes, SHA-256
`D879F55ECBE0B179A6943FF7C8D1C866E5A116646340EDB320713FF801949CF2`.
Two host generation runs produced identical output; the generated Rust is not
hand-patched.