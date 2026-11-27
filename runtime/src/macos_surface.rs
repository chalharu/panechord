//! Private GL drawable host for the macOS backends (GTK and Tao).
//!
//! Binding an NSOpenGLContext to the toolkit-owned contentView hijacks its
//! layer, and the window loses composition (transparent window) once the
//! swapchain is never presented. The context lives on this hidden subview
//! instead; the toolkit view keeps only its own composition. Owned here so
//! the borrowed handle outlives the call. Shared by both backends so the
//! re-parent dance (toolkits recreate the native surface on resize/show)
//! exists exactly once.
use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2_app_kit::NSView;
use raw_window_handle::{AppKitWindowHandle, HandleError, RawWindowHandle, WindowHandle};

pub struct MacosGlHost {
    cell: OnceCell<Retained<NSView>>,
}

impl MacosGlHost {
    pub fn new() -> Self {
        Self {
            cell: OnceCell::new(),
        }
    }

    /// Stable AppKit window handle for `content`, re-parenting the private
    /// hidden host when the toolkit recreates the native surface underneath
    /// us (the old direct-render path re-resolved handles each tick).
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
        Self::new()
    }
}
