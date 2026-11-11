# SDK binding generation

This unpublished tool is a standalone Cargo workspace with its own lockfile.
Official registry `windows-bindgen =0.100.0` generates `../src/owned_bindings.rs`;
**do not hand-edit generated Rust**. Normal product builds use the checked-in output.

## Inputs and provenance

- Read-only Microsoft `windows-rs` revision:
  `a57cc321bfdbac6bb98c488ba49f399da8fcb98a`.
- Source: `crates/tools/reactor/winmd`, 27 WinMD inputs, SDK **2.4.0**.
- 140 filters plus the official generator's default Windows metadata.
- Official composition for Application and Panel; generated implementations for
  IApplicationOverrides, IFrameworkElementOverrides and IXamlMetadataProvider.
- `inputs/PROVENANCE.txt` records the revision and Git blob IDs (SHA-1).
  Extraction uses read-only `git show`; the tool never fetches or modifies upstream.
- Transient `inputs/*.winmd` and local `target/` are ignored. Keep provenance,
  the lockfile and generated Rust in version control.

## Regenerate

From the repository root, using an existing clone containing the pinned revision
and locally cached Cargo dependencies:

```text
cargo run --locked --offline --manifest-path windows/bindgen/Cargo.toml -- <path-to-windows-rs-clone>
```

This runs only the host generator, not a native application/test. It writes inputs,
provenance and Rust; unexpected extra WinMD inputs are rejected. Regenerate twice
and compare output for reproducibility. Output SHA-256:
`2DF1B24D5A3105AE95BBC3174532DDD3BED1EE129624BF3D04E5C66AB90C53C1`
(1,051,950 bytes). The four additional filters expose KeyboardAccelerator
construction, properties, Invoked and its Handled event argument; Panel/Flex
composition is retained.

## Licenses

Retain official tool/binding `MIT OR Apache-2.0` notices and Microsoft SDK terms
for WinMD/runtime payloads. Handwritten product code is MPL-2.0. Setup-derived
assets retain the Microsoft MIT notice in `../assets/LICENSE-MIT.windows-reactor-setup`.