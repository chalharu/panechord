#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("AppKit layout full golden requires macOS");
}

/// Test bridge only: rebuild with `--features test-bridge` (set by
/// verify-macos.sh and the CI golden step). The dump symbols never ship
/// in release or distribution builds by construction.
#[cfg(all(target_os = "macos", not(feature = "test-bridge")))]
fn main() {
    eprintln!("layout full golden requires --features test-bridge");
    std::process::exit(2);
}

/// Full-corpus exact golden: every layout and inline control plus nesting.
/// The corpus itself is shared with the GTK backend test through
/// `panechord_runtime::test_corpus`, so construction parity is enforced by
/// construction. Per-backend golden (`macos/tests/golden/layout_full.json`):
/// native widgets differ by design. Dialog windows are excluded (separate
/// top-level windows) and menus have no layout footprint.
#[cfg(all(target_os = "macos", feature = "test-bridge"))]
fn main() -> Result<(), String> {
    use panechord_macos::run_layout_golden;
    use panechord_runtime::test_corpus;

    run_layout_golden(
        "layout full",
        792,
        760,
        "layout_full.json",
        "MACOS3 PASS layout-full-golden window-relative exact",
        |view, executor| {
            let _corpus = test_corpus::build_full_corpus(view, executor);
        },
    )
}
