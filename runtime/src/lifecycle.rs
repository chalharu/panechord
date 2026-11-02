//! View teardown lifecycle: the SRP split of the former god-`Inner`.
//!
//! Every backend `Inner` used to own the closed flag, final cleanup hooks, the
//! replaceable close-request handler and the synchronous reentrancy gate
//! alongside layout registries, menus, lists and widgets. This component owns
//! exactly that lifecycle slice and nothing backend-specific (only std), so
//! all backends share it instead of triplicating the file. Content registries
//! keep their own teardown; `Inner::shutdown` only orders the two phases
//! (mark closed once, drop content, then run final hooks). Host coordination
//! hooks (close slots, quit routing) stay in `Inner`: they belong to the host
//! relationship, not to the view's own teardown state.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

/// Close/teardown state for one view. See the module docs for the split.
pub struct Lifecycle {
    closed: Cell<bool>,
    on_close: RefCell<Vec<Rc<dyn Fn()>>>,
    on_close_request: RefCell<Option<Rc<dyn Fn()>>>,
    requesting: Cell<bool>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            closed: Cell::new(false),
            on_close: RefCell::default(),
            on_close_request: RefCell::default(),
            requesting: Cell::new(false),
        }
    }
}

/// Reentrancy gate for close requests. Dropping retries a later request.
pub struct RequestGuard<'a> {
    flag: &'a Cell<bool>,
}

impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        self.flag.set(false);
    }
}

/// Panic payload message shared by every backend's final-hook reporting.
/// Backend `panic_error` wrappers add their own callback prefix on top.
pub fn panic_message(panic: &(dyn Any + Send)) -> String {
    panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload")
        .to_owned()
}

impl Lifecycle {
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }
    /// Mark closed exactly once; returns false when already closed so the
    /// content-teardown phase runs a single time.
    pub fn begin_shutdown(&self) -> bool {
        !self.closed.replace(true)
    }
    /// Register a final cleanup hook, or run it immediately on a closed view.
    pub fn on_close(&self, callback: Rc<dyn Fn()>) {
        if self.is_closed() {
            callback();
        } else {
            self.on_close.borrow_mut().push(callback);
        }
    }
    /// Read the application close-request handler, if any.
    pub fn close_request(&self) -> Option<Rc<dyn Fn()>> {
        self.on_close_request.borrow().clone()
    }
    /// Replace the single application close-request handler.
    pub fn set_close_request(&self, callback: Option<Rc<dyn Fn()>>) {
        self.on_close_request.replace(callback);
    }
    /// Enter a close request under the synchronous reentrancy gate. Returns
    /// `None` (request ignored, not queued) when closed or already inside a
    /// request; the guard releases the gate on drop.
    pub fn enter_request(&self) -> Option<RequestGuard<'_>> {
        if self.is_closed() || self.requesting.replace(true) {
            return None;
        }
        Some(RequestGuard {
            flag: &self.requesting,
        })
    }
    /// Run final cleanup hooks after content teardown, capturing panics.
    /// Also releases the close-request handler so post-shutdown installs
    /// cannot resurrect the view.
    pub fn finish_shutdown(&self, report: impl Fn(String)) {
        drop(self.on_close_request.take());
        for callback in self.on_close.take() {
            if let Err(panic) = catch_unwind(AssertUnwindSafe(|| callback())) {
                report(panic_message(&*panic));
            }
        }
    }
}
