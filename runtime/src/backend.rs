//! Host-side backend abstraction: the Dependency Inversion contract.
//!
//! The facade (`panechord::Runner`) used to dispatch through a `macro_rules!`
//! with one arm per backend (`panechord_gtk`, `panechord_tao`, ...). Adding a
//! backend — or a single startup option — meant editing that macro: a textbook
//! Open/Closed violation, with high-level startup policy depending on
//! low-level backend concretions.
//!
//! A backend now implements this one trait. The facade picks the implementation
//! by feature flag (a single type alias) and drives everything else through
//! the trait, so new backends add an `impl` and new startup options extend the
//! trait with a defaulted method instead of touching every call site.

use crate::WindowHost;
use std::rc::Rc;

/// One native backend: its view type, its host type, and the single startup
/// shape (`run_host` plus pre-build option setters) every backend speaks.
/// See `panechord::Runner` for the feature-selected driver.
pub trait Backend {
    /// The backend's concrete view. Must satisfy the shared [`crate::View`]
    /// contract so generic screens substitute across backends (LSP).
    type View: crate::View;
    /// The backend's multi-window host.
    type Host: WindowHost<Self::View>;

    /// Drive the single native loop for this backend.
    fn run_host(
        make: impl FnOnce(Rc<Self::Host>) -> Result<(), String> + 'static,
    ) -> Result<(), String>;
    /// Opt out of (or back into) AppKit automatic window tabbing. No-op where
    /// the platform has no tab bar. Takes effect for windows built after the
    /// call, so invoke it before `run_host`. Defaults to opted out: separate
    /// windows never merge into the system tab bar unless the app opts in.
    fn set_allows_automatic_window_tabbing(enabled: bool);
    /// Stage the initial main-window size in logical points. Consumed once by
    /// the next built window; call before `run_host` builds views.
    fn set_initial_window_size(width: u32, height: u32);
}
