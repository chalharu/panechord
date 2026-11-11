//! SDK-owned WinUI 3 desktop backend with a UI-local executor and close gate.
//! The native renderer owns publication; Reactor is used only for public values
//! and retained DSL snapshots. This crate requires a Windows target.

// Match the AppKit crate: non-Windows workspace builds get an empty backend,
// while selecting this backend still requires a Windows target.
#![cfg(target_os = "windows")]

// Reuse the cache-only build implementation and its original asset paths.
#[cfg(target_os = "windows")]
#[allow(dead_code)]
#[doc(hidden)]
#[path = "../build.rs"]
pub mod deployment;

#[cfg(target_os = "windows")]
mod backend;
#[cfg(target_os = "windows")]
#[path = "owned_driver.rs"]
mod driver;
#[cfg(target_os = "windows")]
#[rustfmt::skip]
#[allow(dead_code, non_snake_case, non_camel_case_types, non_upper_case_globals, clippy::all)]
mod owned_bindings;
#[cfg(target_os = "windows")]
mod host;
#[cfg(target_os = "windows")]
mod view;

// Retain the proven P0 modules verbatim, including their fixture-only methods.
#[cfg(target_os = "windows")]
#[rustfmt::skip]
#[allow(dead_code)]
mod executor;
#[cfg(target_os = "windows")]
#[rustfmt::skip]
#[allow(dead_code)]
mod close;

// Keep the initial diagnostic closure intact; this is not a stable public API.
#[cfg(target_os = "windows")]
#[doc(hidden)]
pub mod smoke;

#[cfg(target_os = "windows")]
pub use backend::Backend;
#[cfg(target_os = "windows")]
pub use driver::run_host;
#[cfg(target_os = "windows")]
pub use host::Host;
#[cfg(target_os = "windows")]
pub use view::{Parent, View, WeakView};

#[cfg(all(test, target_os = "windows"))]
mod tests;
