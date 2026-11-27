//! Headless AppKit bridge smoke for Swift coverage: drives the bridge on
//! the main thread without focus, clipboard, presentation, or golden files,
//! so the macOS CI leg can collect Swift coverage (see
//! `PANECHORD_SWIFT_COVERAGE` in macos/build.rs). Asserts only
//! Rust-observable effects; visuals belong to the golden tests.

#[cfg(target_os = "macos")]
mod native {
    use panechord_macos::View;
    use panechord_runtime::{Binding, ImageFrame, Layout, Style};
    use std::sync::Arc;

    pub fn run_smoke() -> Result<(), String> {
        panechord_macos::run_host(|_host| {
            let view = View::new(Binding::constant("smoke".into()));
            let body = view.layout(&view.root(), "body", Layout::default());
            view.label(&body, "title", Binding::constant("smoke".into()));
            view.style("title", Style::default());
            view.image(&body, "pic", Binding::constant(None));
            let pixels: Arc<[u8]> = Arc::from([255, 0, 0, 255]);
            let frame = ImageFrame::new(1, 1, pixels).map_err(str::to_owned)?;
            view.image(&body, "pic", Binding::constant(Some(frame)));
            assert_eq!(view.image_update_count(), 2);
            assert_eq!(view.text("title").as_deref(), Some("smoke"));
            view.close();
            Ok(())
        })
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        native::run_smoke().unwrap();
        println!("MACOS3 PASS headless-smoke label image empty-image style");
    }
}
