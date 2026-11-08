use crate::{GtkHost, Parent, View};
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
use gtk4::glib::translate::ToGlibPtr;
use gtk4::prelude::*;
use panechord_runtime::{
    Binding, ImageFrame, KeyCode, Session, WindowEvent, WindowHooks, WindowHost, WindowSurface,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::{cell::RefCell, collections::HashSet, rc::Rc};

pub(crate) type ViewportHook = Rc<dyn Fn(f32, f32)>;

struct Surface {
    window: gtk4::Window,
    #[cfg(target_os = "macos")]
    native_view: std::cell::OnceCell<objc2::rc::Retained<objc2_app_kit::NSView>>,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn gdk_macos_surface_get_native_window(
        surface: *mut gtk4::gdk::ffi::GdkSurface,
    ) -> *mut std::ffi::c_void;
}

#[cfg(target_os = "windows")]
unsafe extern "C" {
    fn gdk_win32_surface_get_handle(
        surface: *mut gtk4::gdk::ffi::GdkSurface,
    ) -> *mut std::ffi::c_void;
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
unsafe extern "C" {
    fn gdk_x11_display_get_xdisplay(
        display: *mut gtk4::gdk::ffi::GdkDisplay,
    ) -> *mut std::ffi::c_void;
    fn gdk_x11_surface_get_xid(surface: *mut gtk4::gdk::ffi::GdkSurface) -> std::ffi::c_ulong;
    fn gdk_wayland_display_get_wl_display(
        display: *mut gtk4::gdk::ffi::GdkDisplay,
    ) -> *mut std::ffi::c_void;
    fn gdk_wayland_surface_get_wl_surface(
        surface: *mut gtk4::gdk::ffi::GdkSurface,
    ) -> *mut std::ffi::c_void;
}

impl HasDisplayHandle for Surface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        {
            use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle, XlibDisplayHandle};
            let display = gtk4::prelude::WidgetExt::display(&self.window);
            let raw = if display.type_().name().contains("X11") {
                let pointer = unsafe { gdk_x11_display_get_xdisplay(display.to_glib_none().0) };
                RawDisplayHandle::Xlib(XlibDisplayHandle::new(
                    Some(std::ptr::NonNull::new(pointer).ok_or(HandleError::Unavailable)?),
                    0,
                ))
            } else if display.type_().name().contains("Wayland") {
                let pointer =
                    unsafe { gdk_wayland_display_get_wl_display(display.to_glib_none().0) };
                RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                    std::ptr::NonNull::new(pointer).ok_or(HandleError::Unavailable)?,
                ))
            } else {
                return Err(HandleError::NotSupported);
            };
            Ok(unsafe { DisplayHandle::borrow_raw(raw) })
        }
        #[cfg(target_os = "macos")]
        {
            let display = gtk4::prelude::WidgetExt::display(&self.window);
            if !display.type_().name().contains("Macos") {
                return Err(HandleError::NotSupported);
            }
            Ok(DisplayHandle::appkit())
        }
        #[cfg(target_os = "windows")]
        {
            let display = gtk4::prelude::WidgetExt::display(&self.window);
            if !display.type_().name().contains("Win32") {
                return Err(HandleError::NotSupported);
            }
            Ok(DisplayHandle::windows())
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        )))]
        Err(HandleError::NotSupported)
    }
}

impl HasWindowHandle for Surface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        {
            use raw_window_handle::{RawWindowHandle, WaylandWindowHandle, XlibWindowHandle};
            let surface = self.window.surface().ok_or(HandleError::Unavailable)?;
            let raw = if surface.type_().name().contains("X11") {
                let xid = unsafe { gdk_x11_surface_get_xid(surface.to_glib_none().0) };
                if xid == 0 {
                    return Err(HandleError::Unavailable);
                }
                RawWindowHandle::Xlib(XlibWindowHandle::new(xid))
            } else if surface.type_().name().contains("Wayland") {
                let pointer =
                    unsafe { gdk_wayland_surface_get_wl_surface(surface.to_glib_none().0) };
                RawWindowHandle::Wayland(WaylandWindowHandle::new(
                    std::ptr::NonNull::new(pointer).ok_or(HandleError::Unavailable)?,
                ))
            } else {
                return Err(HandleError::NotSupported);
            };
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
        #[cfg(target_os = "macos")]
        {
            if self.native_view.get().is_none() {
                let surface = self.window.surface().ok_or(HandleError::Unavailable)?;
                if !surface.type_().name().contains("Macos") {
                    return Err(HandleError::NotSupported);
                }
                let pointer =
                    unsafe { gdk_macos_surface_get_native_window(surface.to_glib_none().0) };
                let window = unsafe {
                    objc2::rc::Retained::<objc2_app_kit::NSWindow>::retain(pointer.cast())
                }
                .ok_or(HandleError::Unavailable)?;
                let view = window.contentView().ok_or(HandleError::Unavailable)?;
                let _ = self.native_view.set(view);
            }
            let view = self.native_view.get().ok_or(HandleError::Unavailable)?;
            let pointer =
                std::ptr::NonNull::new(objc2::rc::Retained::as_ptr(view).cast_mut().cast())
                    .ok_or(HandleError::Unavailable)?;
            Ok(unsafe {
                WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::AppKit(
                    raw_window_handle::AppKitWindowHandle::new(pointer),
                ))
            })
        }
        #[cfg(target_os = "windows")]
        {
            let surface = self.window.surface().ok_or(HandleError::Unavailable)?;
            if !surface.type_().name().contains("Win32") {
                return Err(HandleError::NotSupported);
            }
            let pointer = unsafe { gdk_win32_surface_get_handle(surface.to_glib_none().0) };
            let hwnd =
                std::num::NonZeroIsize::new(pointer as isize).ok_or(HandleError::Unavailable)?;
            Ok(unsafe {
                WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::Win32(
                    raw_window_handle::Win32WindowHandle::new(hwnd),
                ))
            })
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        )))]
        Err(HandleError::NotSupported)
    }
}

impl WindowSurface for Surface {
    fn scale_factor(&self) -> f64 {
        f64::from(self.window.scale_factor().max(1))
    }
    fn toggle_fullscreen(&self) {
        self.window.set_fullscreened(!self.window.is_fullscreen());
    }
}

impl WindowHost<View> for GtkHost {
    fn register_view_with_hooks(
        &self,
        session: &Session,
        id: &str,
        view: &View,
        hooks: WindowHooks,
    ) -> Result<(), String> {
        panechord_runtime::UiHost::attach(self, session.clone())?;
        self.register_view_with_hooks(session, id, view, hooks)
    }
}

fn key_code(_key: gtk4::gdk::Key, hardware_code: u32) -> KeyCode {
    #[cfg(target_os = "macos")]
    {
        panechord_runtime::native_keys::macos(hardware_code)
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))]
    {
        hardware_code
            .checked_sub(8)
            .map(panechord_runtime::native_keys::evdev)
            .unwrap_or(KeyCode::Unidentified)
    }
    #[cfg(target_os = "windows")]
    {
        panechord_runtime::native_keys::windows_set1(unsafe {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyW(
                hardware_code,
                windows_sys::Win32::UI::Input::KeyboardAndMouse::MAPVK_VK_TO_VSC_EX,
            )
        })
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )))]
    {
        key_name(_key)
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
fn key_name(key: gtk4::gdk::Key) -> KeyCode {
    if let Some(character) = key.to_unicode() {
        if character.is_ascii_alphabetic() {
            return KeyCode::from_name(&format!("Key{}", character.to_ascii_uppercase()));
        }
        if character.is_ascii_digit() {
            return KeyCode::from_name(&format!("Digit{character}"));
        }
    }
    let name = key.name().map(|name| name.to_string()).unwrap_or_default();
    let canonical = match name.as_str() {
        "Return" => "Enter",
        "Escape" => "Escape",
        "space" => "Space",
        "Tab" | "ISO_Left_Tab" => "Tab",
        "BackSpace" => "Backspace",
        "Up" => "ArrowUp",
        "Down" => "ArrowDown",
        "Left" => "ArrowLeft",
        "Right" => "ArrowRight",
        "Shift_L" => "ShiftLeft",
        "Shift_R" => "ShiftRight",
        "Control_L" => "ControlLeft",
        "Control_R" => "ControlRight",
        "Alt_L" => "AltLeft",
        "Alt_R" => "AltRight",
        "Meta_L" | "Super_L" => "MetaLeft",
        "Meta_R" | "Super_R" => "MetaRight",
        "comma" | "less" => "Comma",
        "period" | "greater" => "Period",
        "slash" | "question" => "Slash",
        "semicolon" | "colon" => "Semicolon",
        "apostrophe" | "quotedbl" => "Quote",
        "bracketleft" | "braceleft" => "BracketLeft",
        "bracketright" | "braceright" => "BracketRight",
        "backslash" | "bar" => "Backslash",
        "minus" | "underscore" => "Minus",
        "equal" => "Equal",
        "plus" => "Equal",
        "grave" | "asciitilde" => "Backquote",
        "Page_Up" => "PageUp",
        "Page_Down" => "PageDown",
        "Caps_Lock" => "CapsLock",
        "Scroll_Lock" => "ScrollLock",
        "Print" => "PrintScreen",
        "KP_Enter" => "NumpadEnter",
        "KP_Add" => "NumpadAdd",
        "KP_Subtract" => "NumpadSubtract",
        "KP_Multiply" => "NumpadMultiply",
        "KP_Divide" => "NumpadDivide",
        "KP_Decimal" => "NumpadDecimal",
        _ => name.as_str(),
    };
    KeyCode::from_name(canonical)
}

impl View {
    pub fn on_surface_ready(
        &self,
        callback: impl FnOnce(Rc<dyn WindowSurface>) -> Result<(), String> + 'static,
    ) -> Result<(), String> {
        if self.0.lifecycle.is_closed() {
            return Err("view is closed".into());
        }
        gtk4::prelude::WidgetExt::realize(&self.0.window);
        callback(Rc::new(Surface {
            window: self.0.window.clone(),
            #[cfg(target_os = "macos")]
            native_view: std::cell::OnceCell::new(),
        }))
    }

    pub fn on_surface_viewport(&self, callback: ViewportHook) {
        self.0.surface_viewport.replace(Some(callback.clone()));
        let (width, height) = self.0.surface_size.get();
        if width > 0.0 && height > 0.0 {
            callback(width, height);
        }
    }

    pub fn on_surface_event(&self, callback: Rc<dyn Fn(&WindowEvent)>) {
        let keys = Rc::new(RefCell::new(HashSet::<KeyCode>::new()));
        let focused = callback.clone();
        let held = keys.clone();
        let signal = self.0.window.connect_is_active_notify(move |window| {
            if !window.is_active() {
                held.borrow_mut().clear();
            }
            focused(&WindowEvent::Focused(window.is_active()));
        });
        self.signal("", &self.0.window, signal);
        let controller = gtk4::EventControllerKey::new();
        controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let pressed = keys.clone();
        let press = callback.clone();
        let signal = controller.connect_key_pressed(move |_, key, hardware_code, _| {
            let key = key_code(key, hardware_code);
            let repeat = !pressed.borrow_mut().insert(key);
            press(&WindowEvent::KeyboardInput {
                key,
                pressed: true,
                repeat,
                synthetic: false,
            });
            gtk4::glib::Propagation::Proceed
        });
        self.signal("", &controller, signal);
        let signal = controller.connect_key_released(move |_, key, hardware_code, _| {
            let key = key_code(key, hardware_code);
            keys.borrow_mut().remove(&key);
            callback(&WindowEvent::KeyboardInput {
                key,
                pressed: false,
                repeat: false,
                synthetic: false,
            });
        });
        self.signal("", &controller, signal);
        self.0.window.add_controller(controller);
    }

    pub fn request_quit(&self) {
        self.0.window.close();
    }

    pub fn surface(&self, parent: &Parent, id: &str, source: Binding<Option<ImageFrame>>) {
        // Master parity: a single fill Picture, no black DrawingArea under
        // an Overlay. The overlay's black child painted every draw and the
        // overlay-sized Picture could not reliably cover it (usually black
        // with brief correct flashes); master sets a lone Picture as the
        // window child and polls its allocation per tick.
        let picture = gtk4::Picture::new();
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_can_shrink(true);
        let weak = Rc::downgrade(&self.0);
        picture.add_tick_callback(move |widget, _| {
            if let Some(inner) = weak.upgrade() {
                let allocation = widget.allocation();
                let size = (
                    allocation.width().max(0) as f32,
                    allocation.height().max(0) as f32,
                );
                if inner.surface_size.get() != size {
                    inner.surface_size.set(size);
                    let callback = inner.surface_viewport.borrow().clone();
                    if let Some(callback) = callback {
                        callback(size.0, size.1);
                    }
                }
            }
            gtk4::glib::ControlFlow::Continue
        });
        self.mount_native(parent, id, &picture);
        self.bind_native(id, source, &picture, |picture, frame| {
            let texture = frame.map(|frame| {
                gtk4::gdk::MemoryTexture::new(
                    frame.width() as i32,
                    frame.height() as i32,
                    gtk4::gdk::MemoryFormat::R8g8b8a8,
                    &gtk4::glib::Bytes::from_owned(frame.pixels().clone()),
                    frame.width() as usize * 4,
                )
            });
            picture.set_paintable(texture.as_ref());
        });
    }
}
