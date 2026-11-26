#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("AppKit layout golden requires macOS");
}

/// Test bridge only: rebuild with `--features test-bridge` (set by
/// verify-macos.sh and the CI golden step). The dump symbols never ship
/// in release or distribution builds by construction.
#[cfg(all(target_os = "macos", not(feature = "test-bridge")))]
fn main() {
    eprintln!("layout golden requires --features test-bridge");
    std::process::exit(2);
}

/// Minimal shared-corpus golden: 3 equal tracks, monospace labels.
/// Same shape as the GTK `native_layout_golden_exact` JSON.
#[cfg(all(target_os = "macos", feature = "test-bridge"))]
fn main() -> Result<(), String> {
    use panechord_macos::{View, run_layout_golden};
    use panechord_runtime::{Binding, Layout, Style, TextRole, TextStyle};
    use std::rc::Rc;

    run_layout_golden(
        "layout golden",
        660,
        400,
        "tests/golden/layout_exact.json",
        "MACOS3 PASS layout-golden window-relative exact",
        |view: &View, _executor: &Rc<dyn panechord_runtime::Executor>| {
            let grid = view.layout(
                &view.root(),
                "grid",
                Layout::Grid {
                    columns: 3,
                    gap: 6.0,
                },
            );
            for (index, text) in ["a", "b", "c"].into_iter().enumerate() {
                let id = format!("g{index}");
                view.label(&grid, &id, Binding::constant(text.into()));
                view.style(
                    &id,
                    Style {
                        text: Some(TextStyle {
                            role: TextRole::Code,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                );
            }
        },
    )
}
