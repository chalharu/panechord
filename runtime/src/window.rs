use crate::{KeyCode, Session, UiHost, ViewCore, WeakViewHandle, WindowHooks};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum WindowEvent {
    Focused(bool),
    KeyboardInput {
        key: KeyCode,
        pressed: bool,
        repeat: bool,
        synthetic: bool,
    },
}

pub trait WindowSurface: HasDisplayHandle + HasWindowHandle {
    fn scale_factor(&self) -> f64;
    fn toggle_fullscreen(&self);
}

pub trait WindowHost<V>: UiHost {
    fn register_view_with_hooks(
        &self,
        session: &Session,
        id: &str,
        view: &V,
        hooks: WindowHooks,
    ) -> Result<(), String>;
    /// Register a window with no application work to coordinate: shutdown
    /// confirms immediately, nothing cancels or drains, and closing the
    /// window closes the view. This is the common shape, so it keeps the
    /// short name; the hooks-taking variant carries the suffix. Every
    /// backend speaks this, so screens never branch on backend features
    /// to show a view.
    fn register_view(&self, session: &Session, id: &str, view: &V) -> Result<(), String>
    where
        V: ViewCore,
    {
        let weak = view.downgrade();
        self.register_view_with_hooks(
            session,
            id,
            view,
            WindowHooks {
                confirm: Rc::new(|| Box::pin(async { true })),
                cancel: Rc::new(|| {}),
                drain: Rc::new(|| Box::pin(async {})),
                close: Rc::new(move || {
                    if let Some(view) = weak.upgrade() {
                        view.close();
                    }
                }),
            },
        )
    }
}
