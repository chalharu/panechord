pub use panechord_macros::{commands, owned_view, try_owned_view, try_view, view};
pub use panechord_runtime::*;

#[cfg(any(
    all(feature = "gtk", feature = "tao"),
    all(feature = "gtk", feature = "macos"),
    all(feature = "tao", feature = "macos"),
    all(feature = "windows", feature = "gtk"),
    all(feature = "windows", feature = "tao"),
    all(feature = "windows", feature = "macos")
))]
compile_error!("select exactly one Panechord native backend");

#[cfg(feature = "gtk")]
pub use panechord_gtk as gtk;
#[cfg(feature = "macos")]
pub use panechord_macos as macos;
#[cfg(all(
    feature = "native",
    not(any(
        feature = "gtk",
        feature = "tao",
        feature = "macos",
        feature = "windows"
    ))
))]
pub use panechord_native as native;
#[cfg(feature = "tao")]
pub use panechord_tao as tao;
#[cfg(feature = "windows")]
pub use panechord_windows as windows;

/// Startup builder: pre-build window options flow into the backend before the
/// single `run_host` loop starts. Views are built inside `run`, so every
/// branch stays unit-testable on any platform.
#[cfg(any(
    feature = "native",
    feature = "gtk",
    feature = "tao",
    feature = "macos",
    feature = "windows"
))]
pub struct Runner;
#[cfg(any(
    feature = "native",
    feature = "gtk",
    feature = "tao",
    feature = "macos",
    feature = "windows"
))]
impl Runner {
    pub fn builder() -> RunnerBuilder {
        RunnerBuilder {
            automatic_window_tabbing: None,
            initial_window_size: None,
        }
    }
}
#[cfg(any(
    feature = "native",
    feature = "gtk",
    feature = "tao",
    feature = "macos",
    feature = "windows"
))]
pub struct RunnerBuilder {
    automatic_window_tabbing: Option<bool>,
    initial_window_size: Option<(u32, u32)>,
}
/// macOS-only builder options, mirroring tao's `WindowBuilderExtMacOS`.
/// Available only on Apple targets; other platforms have no window tabbing.
/// Frontend crates call this behind `#[cfg(target_os = "macos")]` (see the
/// sanctioned exception in check_frontend_policy.py): the cfg is the
/// platform boundary itself, not a backend leak.
#[cfg(all(
    target_os = "macos",
    any(
        feature = "native",
        feature = "gtk",
        feature = "tao",
        feature = "macos",
        feature = "windows"
    )
))]
pub trait RunnerBuilderExtMacOS: Sized {
    /// Opt out of (or back into) AppKit automatic window tabbing. Takes
    /// effect for windows created after `run`. When unset, each backend
    /// keeps its default (the AppKit host opts out; tao/gtk keep the
    /// system default).
    fn allows_automatic_window_tabbing(self, enabled: bool) -> Self;
}

#[cfg(all(
    target_os = "macos",
    any(
        feature = "native",
        feature = "gtk",
        feature = "tao",
        feature = "macos",
        feature = "windows"
    )
))]
impl RunnerBuilderExtMacOS for RunnerBuilder {
    fn allows_automatic_window_tabbing(mut self, enabled: bool) -> Self {
        self.automatic_window_tabbing = Some(enabled);
        self
    }
}

/// Feature-selected backend. Downstream screens name the macro backend path
/// through this module (`panechord::backend`), never the concrete backend
/// crates: native backend selection belongs to Panechord. Combinations of
/// `gtk`/`tao`/`macos`/`windows` are rejected above; alongside `native`, the explicit
/// backend wins and `native` (the platform default) stays silent.
/// `SelectedBackend` below derives from this module, so adding a backend
/// touches exactly one alias plus its [`panechord_runtime::Backend`] impl.
#[cfg(feature = "gtk")]
pub use panechord_gtk as backend;
#[cfg(feature = "macos")]
pub use panechord_macos as backend;
#[cfg(all(
    feature = "native",
    not(any(
        feature = "gtk",
        feature = "tao",
        feature = "macos",
        feature = "windows"
    ))
))]
pub use panechord_native as backend;
#[cfg(feature = "tao")]
pub use panechord_tao as backend;
#[cfg(feature = "windows")]
pub use panechord_windows as backend;

/// Feature-selected backend implementation behind the [`backend`] module:
/// startup policy below stays generic, so new backends and new builder
/// options never touch the dispatch itself (Open/Closed). High-level code
/// depends on the trait, not on each backend's free functions (Dependency
/// Inversion).
#[cfg(any(
    feature = "native",
    feature = "gtk",
    feature = "tao",
    feature = "macos",
    feature = "windows"
))]
type SelectedBackend = backend::Backend;

#[cfg(any(
    feature = "native",
    feature = "gtk",
    feature = "tao",
    feature = "macos",
    feature = "windows"
))]
impl RunnerBuilder {
    /// Restore the remembered main-window size (logical points) for the
    /// first window `run` builds. Out-of-range values keep the default.
    pub fn initial_window_size(mut self, width: u32, height: u32) -> Self {
        self.initial_window_size = Some((width, height));
        self
    }

    pub fn run(
        self,
        make: impl FnOnce(
            std::rc::Rc<dyn WindowHost<<SelectedBackend as panechord_runtime::Backend>::View>>,
        ) -> Result<(), String>
        + 'static,
    ) -> Result<(), String> {
        // One dispatch shape for every backend, driven through the
        // `panechord_runtime::Backend` trait: `run_host` and the pre-build
        // option setters. A new builder option extends the trait; a new
        // backend implements it.
        use panechord_runtime::Backend as _;
        let tabbing = self.automatic_window_tabbing;
        let size = self.initial_window_size;
        SelectedBackend::run_host(move |host| {
            if let Some(enabled) = tabbing {
                SelectedBackend::set_allows_automatic_window_tabbing(enabled);
            }
            if let Some((width, height)) = size {
                SelectedBackend::set_initial_window_size(width, height);
            }
            make(host)
        })
    }
}
