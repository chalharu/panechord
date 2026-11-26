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
/// Mirrors `native_layout_full_golden_exact` in the GTK backend test for
/// test: same IDs, styles, sizes and construction order. Per-backend
/// golden (`macos/tests/golden/layout_full.json`): native widgets differ
/// by design, while `tests/golden/layout_exact.json` guards the shared
/// cross-backend-identical subset. Dialog windows are excluded (separate
/// top-level windows) and menus have no layout footprint.
#[cfg(all(target_os = "macos", feature = "test-bridge"))]
fn main() -> Result<(), String> {
    use panechord_macos::View;
    use panechord_runtime::{Binding, Session, UiHost as _, WindowHost as _, test_corpus};
    use std::{cell::Cell, rc::Rc, time::Instant};

    panechord_macos::set_initial_window_size(792, 760);
    let passed = Rc::new(Cell::new(false));
    let finished = passed.clone();
    // Blocking file IO stays in synchronous code: nothing inside the async
    // task touches the filesystem.
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("layout_full.json");
    let expected = std::fs::read_to_string(&golden_path).ok();
    let dumped = Rc::new(std::cell::RefCell::new(None::<String>));
    let written = dumped.clone();
    let result = panechord_macos::run_host(move |host| {
        let session = Session::new();
        host.attach(session.clone())?;
        let executor = host.executor();
        let view = View::new(Binding::constant("layout full".into()));
        let _corpus = test_corpus::build_full_corpus(&view, &executor);
        let owner = view.clone();
        executor.spawn(Box::pin(async move {
            let activated = Instant::now();
            while !owner.is_key_window() {
                assert!(
                    activated.elapsed() < std::time::Duration::from_secs(10),
                    "native window did not become active"
                );
                futures_timer::Delay::new(std::time::Duration::from_millis(8)).await;
            }
            let first = owner.dump_layout_json().expect("layout must be mapped");
            let second = owner.dump_layout_json().expect("layout must be stable");
            assert_eq!(first, second, "dump_layout must be deterministic");
            written.borrow_mut().replace(first.clone());
            match &expected {
                Some(text) => assert_eq!(
                    first,
                    text.trim(),
                    "layout drift: OSX_GOLDEN_ACTUAL={first}"
                ),
                None => panic!("missing golden; OSX_GOLDEN_ACTUAL={first}"),
            }
            println!("MACOS3 PASS layout-full-golden window-relative exact");
            owner.close();
            finished.set(true);
        }));
        host.register_view(&session, "main", &view)?;
        Ok(())
    });
    result?;
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        if let Some(json) = dumped.borrow().clone() {
            std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
            std::fs::write(&golden_path, format!("{json}\n")).unwrap();
        }
    }
    if passed.get() {
        Ok(())
    } else {
        Err("macOS layout full golden did not complete".into())
    }
}
