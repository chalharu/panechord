//! Private GL drawable host for the macOS backends (GTK and Tao).
//!
//! Binding an NSOpenGLContext to the toolkit-owned contentView hijacks its
//! layer, and the window loses composition (transparent window) once the
//! swapchain is never presented. The context lives on this hidden subview
//! instead; the toolkit view keeps only its own composition. Owned here so
//! the borrowed handle outlives the call.
//!
//! This lives in its own leaf crate — not in `panechord-runtime` — so the
//! shared-state core never depends on a platform SDK. Only the macOS
//! surface paths of the backends depend on this crate.
#![cfg(target_os = "macos")]
use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2_app_kit::NSView;
use raw_window_handle::{AppKitWindowHandle, HandleError, RawWindowHandle, WindowHandle};

pub struct MacosGlHost {
    cell: OnceCell<Retained<NSView>>,
}

impl MacosGlHost {
    /// Stable AppKit window handle for `content`, re-parenting the private
    /// hidden host when the toolkit recreates the native surface underneath
    /// us.
    pub fn handle<'a>(
        &'a self,
        content: &Retained<NSView>,
    ) -> Result<WindowHandle<'a>, HandleError> {
        let mtm = objc2::MainThreadMarker::new().ok_or(HandleError::Unavailable)?;
        let host = self.cell.get_or_init(|| {
            let view = NSView::new(mtm);
            view.setHidden(true);
            view
        });
        let reparent = unsafe { host.superview() }
            .map(|parent| {
                !std::ptr::eq(
                    Retained::as_ptr(&parent).cast::<std::ffi::c_void>(),
                    Retained::as_ptr(content).cast::<std::ffi::c_void>(),
                )
            })
            .unwrap_or(true);
        if reparent {
            host.removeFromSuperview();
            host.setFrame(content.bounds());
            host.setAutoresizingMask(
                objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable
                    | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            content.addSubview(host);
        }
        let pointer = std::ptr::NonNull::new(Retained::as_ptr(host).cast_mut().cast())
            .ok_or(HandleError::Unavailable)?;
        Ok(unsafe {
            WindowHandle::borrow_raw(RawWindowHandle::AppKit(AppKitWindowHandle::new(pointer)))
        })
    }
}

impl Default for MacosGlHost {
    fn default() -> Self {
        Self {
            cell: OnceCell::new(),
        }
    }
}
