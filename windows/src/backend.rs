use crate::{Host, View, driver};
use std::rc::Rc;

/// Full runtime Backend contract over the retained twelve-capability View.
pub struct Backend;

impl panechord_runtime::Backend for Backend {
    type View = View;
    type Host = Host;
    const NAME: &'static str = "windows";

    fn run_host(make: impl FnOnce(Rc<Host>) -> Result<(), String> + 'static) -> Result<(), String> {
        driver::run_host(make)
    }

    fn set_allows_automatic_window_tabbing(_enabled: bool) {
        // WinUI has no AppKit automatic window tabbing.
    }

    fn set_initial_window_size(width: u32, height: u32) {
        driver::set_initial_window_size(width, height);
    }
}
