use crate::{NativeWindow, NativeWindowEvent, Parent, TaoHost, View};
use panechord_runtime::{
    Binding, ImageFrame, KeyCode, Session, WindowEvent, WindowHooks, WindowHost, WindowSurface,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::{rc::Rc, sync::Arc};

struct Surface {
    window: Arc<NativeWindow>,
    /// Private GL drawable host (macOS): same reason as the GTK backend —
    /// binding an NSOpenGLContext to the toolkit-owned view hijacks its
    /// layer while the readback path never presents, breaking window
    /// composition.
    #[cfg(target_os = "macos")]
    gl_host: panechord_surface_macos::MacosGlHost,
}

impl HasDisplayHandle for Surface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.window.display_handle()
    }
}

impl HasWindowHandle for Surface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        #[cfg(not(target_os = "macos"))]
        {
            self.window.window_handle()
        }
        #[cfg(target_os = "macos")]
        {
            let borrowed = self
                .window
                .window_handle()
                .map_err(|_| HandleError::Unavailable)?;
            let raw_window_handle::RawWindowHandle::AppKit(appkit) = borrowed.as_raw() else {
                return Err(HandleError::NotSupported);
            };
            let view = unsafe {
                objc2::rc::Retained::<objc2_app_kit::NSView>::retain(
                    appkit.ns_view.cast::<objc2_app_kit::NSView>().as_ptr(),
                )
            }
            .ok_or(HandleError::Unavailable)?;
            let ns_window = view.window().ok_or(HandleError::Unavailable)?;
            let content = ns_window.contentView().ok_or(HandleError::Unavailable)?;
            self.gl_host.handle(&content)
        }
    }
}

impl WindowSurface for Surface {
    fn scale_factor(&self) -> f64 {
        self.window.scale_factor()
    }

    fn toggle_fullscreen(&self) {
        self.window
            .set_fullscreen(if self.window.fullscreen().is_some() {
                None
            } else {
                Some(crate::NativeFullscreen::Borderless(None))
            });
    }
}

impl WindowHost<View> for TaoHost {
    fn register_view_with_hooks(
        &self,
        session: &Session,
        id: &str,
        view: &View,
        hooks: WindowHooks,
    ) -> Result<(), String> {
        TaoHost::register_view_with_hooks(self, session, id, view, hooks)
    }
}

impl View {
    fn key_code(code: crate::NativeKeyCode) -> KeyCode {
        // `NativeKeyCode` (winit) and `KeyCode` (runtime) share physical
        // variant names for every mapped code, so the canonical name table
        // in `runtime::key` is the single source of truth: derive through
        // it instead of duplicating the ~155-variant list here. Unmapped
        // codes (including `Unidentified(..)`) fall through to
        // `KeyCode::Unidentified`, exactly like the old `_` arm.
        KeyCode::from_name(&format!("{code:?}"))
    }

    pub fn on_surface_ready(
        &self,
        callback: impl FnOnce(Rc<dyn WindowSurface>) -> Result<(), String> + 'static,
    ) -> Result<(), String> {
        self.on_native_window(move |window| {
            callback(Rc::new(Surface {
                window,
                #[cfg(target_os = "macos")]
                gl_host: panechord_surface_macos::MacosGlHost::default(),
            }))
        })
    }

    pub fn on_surface_viewport(&self, callback: Rc<dyn Fn(f32, f32)>) {
        self.on_native_viewport(callback);
    }

    pub fn on_surface_event(&self, callback: Rc<dyn Fn(&WindowEvent)>) {
        self.on_native_event(Rc::new(move |event| {
            let event = match event {
                NativeWindowEvent::Focused(focused) => WindowEvent::Focused(*focused),
                NativeWindowEvent::KeyboardInput {
                    event,
                    is_synthetic,
                    ..
                } => WindowEvent::KeyboardInput {
                    key: Self::key_code(event.physical_key),
                    pressed: event.state == crate::NativeElementState::Pressed,
                    repeat: event.repeat,
                    synthetic: *is_synthetic,
                },
                _ => return,
            };
            callback(&event);
        }));
    }

    pub fn surface(&self, parent: &Parent, id: &str, source: Binding<Option<ImageFrame>>) {
        let owner = Rc::downgrade(&self.0);
        self.mount_bound(
            parent,
            id,
            source,
            Rc::new(move |frame| {
                let (width, height) = owner
                    .upgrade()
                    .map_or((1.0, 1.0), |inner| inner.surface_bounds.get());
                element(frame, width, height)
            }),
        );
    }
}

pub(super) fn element(frame: Option<ImageFrame>, width: f32, height: f32) -> crate::Element {
    use iced::{Length, widget};
    let content: crate::Element = if let Some(frame) = frame {
        widget::image(widget::image::Handle::from_rgba(
            frame.width(),
            frame.height(),
            frame.pixels().as_ref().to_vec(),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        widget::Space::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };
    widget::container(content)
        .width(width.max(1.0))
        .height(height.max(1.0))
        .style(|_| widget::container::Style {
            background: Some(iced::Color::BLACK.into()),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod input_tests {
    use super::*;
    #[test]
    fn native_tao_codes_map_directly_to_common_physical_keys() {
        for (native, common) in [
            (crate::NativeKeyCode::KeyV, KeyCode::KeyV),
            (crate::NativeKeyCode::ShiftRight, KeyCode::ShiftRight),
            (crate::NativeKeyCode::F35, KeyCode::F35),
            (crate::NativeKeyCode::IntlYen, KeyCode::IntlYen),
            // Dropped by the old hand list, now mapped through the runtime table.
            (crate::NativeKeyCode::Fn, KeyCode::Fn),
            // Still unmapped names fall through to Unidentified.
            (
                crate::NativeKeyCode::Unidentified(tao::keyboard::NativeKeyCode::Unidentified),
                KeyCode::Unidentified,
            ),
        ] {
            assert_eq!(View::key_code(native), common);
        }
    }
}
