#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("AppKit layout golden requires macOS");
}

/// Fixed-corpus window-relative layout golden: 3 equal tracks with a
/// divisible gap, monospace labels (SF Mono via the Code role) so the
/// golden tracks panechord regressions, not system font resolution.
/// Same shape as the GTK `native_layout_golden_exact` JSON.
#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use panechord_macos::View;
    use panechord_runtime::{
        Binding, Layout, Session, Style, TextRole, TextStyle, UiHost as _, WindowHost as _,
    };
    use std::{cell::Cell, rc::Rc, time::Instant};

    panechord_macos::set_initial_window_size(660, 400);
    let passed = Rc::new(Cell::new(false));
    let finished = passed.clone();
    let result = panechord_macos::run_host(move |host| {
        let session = Session::new();
        host.attach(session.clone())?;
        let executor = host.executor();
        let view = View::new(Binding::constant("layout golden".into()));
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
            let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("golden")
                .join("layout_exact.json");
            if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
                std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
                std::fs::write(&golden_path, format!("{first}\n")).unwrap();
            } else if let Ok(expected) = std::fs::read_to_string(&golden_path) {
                assert_eq!(
                    first,
                    expected.trim(),
                    "layout drift: OSX_GOLDEN_ACTUAL={first}"
                );
            } else {
                panic!("missing golden; OSX_GOLDEN_ACTUAL={first}");
            }
            println!("MACOS3 PASS layout-golden window-relative exact");
            owner.close();
            finished.set(true);
        }));
        host.register_view(&session, "main", &view)?;
        Ok(())
    });
    result?;
    if passed.get() {
        Ok(())
    } else {
        Err("macOS layout golden did not complete".into())
    }
}
