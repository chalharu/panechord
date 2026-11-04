use iced::{Event, Point, keyboard as k, mouse};
use tao::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard as tk,
};

/// Native scroll kinds across event sources: tao wheel events (all
/// platforms) and GTK scroll events (Linux, where tao renders through a
/// GTK drawing area). Discrete clicks carry ±1 lines; smooth touchpad and
/// high-resolution wheel deltas carry raw pixel-scale (dx, dy).
///
/// Only consumed by the Linux scroll bridge (plus tests everywhere), so
/// other backends silence the dead-code lint instead of deleting the
/// shared mapping.
#[cfg_attr(
    not(any(
        test,
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )),
    allow(dead_code)
)]
pub(super) enum NativeScroll {
    Up,
    Down,
    Left,
    Right,
    Smooth(f64, f64),
}

/// Map a native scroll to an iced wheel delta. Discrete clicks become ±1
/// lines; smooth deltas pass through as raw pixels — relative motion is
/// never scale-divided (only absolute cursor positions are). Shared by the
/// tao event converter and the Linux GTK scroll-event bridge so both agree
/// on direction and scale. Dropping `Smooth` here silently kills touchpad
/// scrolling, the dominant scroll source on modern Linux.
#[cfg_attr(
    not(any(
        test,
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )),
    allow(dead_code)
)]
pub(super) fn scroll_delta(kind: NativeScroll) -> mouse::ScrollDelta {
    match kind {
        NativeScroll::Up => mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
        NativeScroll::Down => mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
        NativeScroll::Left => mouse::ScrollDelta::Lines { x: 1.0, y: 0.0 },
        NativeScroll::Right => mouse::ScrollDelta::Lines { x: -1.0, y: 0.0 },
        NativeScroll::Smooth(x, y) => mouse::ScrollDelta::Pixels {
            x: x as f32,
            y: y as f32,
        },
    }
}

/// Merge a wheel delta into a pending coalesced scroll. Trackpads emit
/// dozens of ticks per second and every pump rebuilds the whole widget
/// tree (the Input tab mounts every binding row), so ticks accumulate
/// here and flush once per frame. Same units add up; mixing lines with
/// pixels refuses (the caller flushes first) so units never corrupt.
pub(super) fn merge_wheel(
    pending: Option<mouse::ScrollDelta>,
    next: mouse::ScrollDelta,
) -> Option<mouse::ScrollDelta> {
    match (pending, next) {
        (None, next) => Some(next),
        (
            Some(mouse::ScrollDelta::Lines { x: ax, y: ay }),
            mouse::ScrollDelta::Lines { x: bx, y: by },
        ) => Some(mouse::ScrollDelta::Lines {
            x: ax + bx,
            y: ay + by,
        }),
        (
            Some(mouse::ScrollDelta::Pixels { x: ax, y: ay }),
            mouse::ScrollDelta::Pixels { x: bx, y: by },
        ) => Some(mouse::ScrollDelta::Pixels {
            x: ax + bx,
            y: ay + by,
        }),
        _ => None,
    }
}

/// Wrap a (possibly coalesced) wheel delta as the iced event the driver
/// delivers on flush.
pub(super) fn wheel_event(delta: mouse::ScrollDelta) -> Event {
    Event::Mouse(mouse::Event::WheelScrolled { delta })
}

/// Whether a pump dirtied anything: iced asked for a redraw, the tree is
/// outdated, or a message awaits dispatch (dispatch may invalidate through
/// bindings). Clean events — a wheel tick that moved nothing, a hovered
/// cursor over static content — must not schedule a full rebuild+present.
pub(super) fn needs_redraw(
    state: &iced_winit::runtime::user_interface::State,
    has_messages: bool,
) -> bool {
    use iced_winit::core::window::RedrawRequest;
    has_messages
        || match state {
            iced_winit::runtime::user_interface::State::Outdated => true,
            iced_winit::runtime::user_interface::State::Updated { redraw_request, .. } => {
                *redraw_request != RedrawRequest::Wait
            }
        }
}
pub(super) fn key(key: &tk::Key<'_>) -> k::Key {
    use k::key::Named;
    if let tk::Key::Character(text) = key {
        return k::Key::Character((*text).into());
    }
    k::Key::Named(match key {
        tk::Key::Space => Named::Space,
        tk::Key::Enter => Named::Enter,
        tk::Key::Tab => Named::Tab,
        tk::Key::Backspace => Named::Backspace,
        tk::Key::Delete => Named::Delete,
        tk::Key::Escape => Named::Escape,
        tk::Key::ArrowLeft => Named::ArrowLeft,
        tk::Key::ArrowRight => Named::ArrowRight,
        tk::Key::ArrowUp => Named::ArrowUp,
        tk::Key::ArrowDown => Named::ArrowDown,
        tk::Key::Home => Named::Home,
        tk::Key::End => Named::End,
        tk::Key::PageUp => Named::PageUp,
        tk::Key::PageDown => Named::PageDown,
        tk::Key::Shift => Named::Shift,
        tk::Key::Control => Named::Control,
        tk::Key::Alt => Named::Alt,
        tk::Key::Super => Named::Super,
        tk::Key::Copy => Named::Copy,
        tk::Key::Cut => Named::Cut,
        tk::Key::Paste => Named::Paste,
        _ => return k::Key::Unidentified,
    })
}
pub(super) fn convert(
    event: &WindowEvent<'_>,
    scale: f64,
    offset: Point,
    cursor: &mut mouse::Cursor,
    modifiers: &mut k::Modifiers,
) -> Option<Event> {
    Some(match event {
        WindowEvent::CursorMoved { position, .. } => {
            let position = Point::new(
                position.x as f32 / scale as f32 - offset.x,
                position.y as f32 / scale as f32 - offset.y,
            );
            *cursor = if position.y < 0.0 || position.x < 0.0 {
                mouse::Cursor::Unavailable
            } else {
                mouse::Cursor::Available(position)
            };
            Event::Mouse(mouse::Event::CursorMoved { position })
        }
        WindowEvent::CursorLeft { .. } => {
            *cursor = mouse::Cursor::Unavailable;
            Event::Mouse(mouse::Event::CursorLeft)
        }
        WindowEvent::MouseInput { state, button, .. } => {
            let button = match button {
                MouseButton::Left => mouse::Button::Left,
                MouseButton::Right => mouse::Button::Right,
                MouseButton::Middle => mouse::Button::Middle,
                MouseButton::Other(n) => mouse::Button::Other(*n),
                _ => return None,
            };
            Event::Mouse(if *state == ElementState::Pressed {
                mouse::Event::ButtonPressed(button)
            } else {
                mouse::Event::ButtonReleased(button)
            })
        }
        WindowEvent::MouseWheel { delta, .. } => Event::Mouse(mouse::Event::WheelScrolled {
            delta: match delta {
                MouseScrollDelta::LineDelta(x, y) => mouse::ScrollDelta::Lines { x: *x, y: *y },
                // Pixel deltas (trackpads, high-resolution wheels) pass
                // through unscaled, matching the reference iced_winit
                // conversion. They are relative motion, not positions: the
                // tao value already accounts for the display, and iced
                // scrolls Pixels 1:1. Dividing by the scale factor (as cursor
                // positions require) halves trackpad speed on Retina and
                // makes long scroll views feel dead.
                MouseScrollDelta::PixelDelta(p) => mouse::ScrollDelta::Pixels {
                    x: p.x as f32,
                    y: p.y as f32,
                },
                _ => return None,
            },
        }),
        WindowEvent::ModifiersChanged(m) => {
            *modifiers = k::Modifiers::empty();
            modifiers.set(k::Modifiers::SHIFT, m.shift_key());
            modifiers.set(k::Modifiers::CTRL, m.control_key());
            modifiers.set(k::Modifiers::ALT, m.alt_key());
            modifiers.set(k::Modifiers::LOGO, m.super_key());
            Event::Keyboard(k::Event::ModifiersChanged(*modifiers))
        }
        WindowEvent::KeyboardInput {
            event,
            is_synthetic: false,
            ..
        } => {
            let key = key(&event.key_without_modifiers());
            let modified_key = self::key(&event.logical_key);
            let physical_key = k::key::Physical::Unidentified(k::key::NativeCode::Unidentified);
            let location = match event.location {
                tk::KeyLocation::Left => k::Location::Left,
                tk::KeyLocation::Right => k::Location::Right,
                tk::KeyLocation::Numpad => k::Location::Numpad,
                _ => k::Location::Standard,
            };
            Event::Keyboard(if event.state == ElementState::Pressed {
                k::Event::KeyPressed {
                    key,
                    modified_key,
                    physical_key,
                    location,
                    modifiers: *modifiers,
                    // Linux and macOS deliver ordinary text via ReceivedImeText too.
                    text: if cfg!(any(
                        target_os = "linux",
                        target_os = "macos",
                        target_os = "freebsd",
                        target_os = "netbsd",
                        target_os = "openbsd",
                        target_os = "dragonfly"
                    )) {
                        None
                    } else {
                        event
                            .text
                            .filter(|t| !t.chars().any(char::is_control))
                            .map(Into::into)
                    },
                    repeat: event.repeat,
                }
            } else {
                k::Event::KeyReleased {
                    key,
                    modified_key,
                    physical_key,
                    location,
                    modifiers: *modifiers,
                }
            })
        }
        WindowEvent::ReceivedImeText(text) => {
            Event::InputMethod(iced::advanced::input_method::Event::Commit(text.clone()))
        }
        WindowEvent::Focused(focused) => {
            if !focused {
                *modifiers = k::Modifiers::empty();
            }
            Event::Window(if *focused {
                iced::window::Event::Focused
            } else {
                iced::window::Event::Unfocused
            })
        }
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(deprecated)]
    fn hidpi_and_native_menu_offset_and_commit() {
        let mut cursor = mouse::Cursor::Unavailable;
        let mut modifiers = k::Modifiers::empty();
        convert(
            &WindowEvent::CursorMoved {
                device_id: unsafe { tao::event::DeviceId::dummy() },
                position: tao::dpi::PhysicalPosition::new(80.0, 120.0),
                modifiers: tk::ModifiersState::empty(),
            },
            2.0,
            Point::new(0.0, 24.0),
            &mut cursor,
            &mut modifiers,
        );
        assert_eq!(cursor.position(), Some(Point::new(40.0, 36.0)));
        assert_eq!(
            convert(
                &WindowEvent::ReceivedImeText("日本語".into()),
                1.0,
                Point::ORIGIN,
                &mut cursor,
                &mut modifiers
            ),
            Some(Event::InputMethod(
                iced::advanced::input_method::Event::Commit("日本語".into())
            ))
        );
    }
    #[test]
    fn limits_are_explicit() {
        let caps = crate::View::input_capabilities();
        assert!(caps.system_clipboard && caps.ime_commit);
        assert!(!caps.ime_preedit && !caps.physical_keys && !caps.os_accessibility);
        assert_eq!(
            key(&tk::Key::Backspace),
            k::Key::Named(k::key::Named::Backspace)
        );
    }

    #[test]
    fn scroll_mapping_covers_discrete_and_smooth_sources() {
        // Discrete clicks: ±1 lines, matching the pre-existing GTK bridge.
        assert_eq!(
            scroll_delta(NativeScroll::Up),
            mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }
        );
        assert_eq!(
            scroll_delta(NativeScroll::Down),
            mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }
        );
        assert_eq!(
            scroll_delta(NativeScroll::Left),
            mouse::ScrollDelta::Lines { x: 1.0, y: 0.0 }
        );
        assert_eq!(
            scroll_delta(NativeScroll::Right),
            mouse::ScrollDelta::Lines { x: -1.0, y: 0.0 }
        );
        // Smooth (touchpad/high-resolution wheel): raw pixel deltas pass
        // through with their sign, so Linux touchpads scroll at all and in
        // the same direction as discrete wheels (positive = down/right).
        assert_eq!(
            scroll_delta(NativeScroll::Smooth(0.0, 24.0)),
            mouse::ScrollDelta::Pixels { x: 0.0, y: 24.0 }
        );
        assert_eq!(
            scroll_delta(NativeScroll::Smooth(-3.5, -12.25)),
            mouse::ScrollDelta::Pixels { x: -3.5, y: -12.25 }
        );
    }

    /// tao wheel-event conversion: discrete lines pass through, and precise
    /// (trackpad/high-resolution) pixel deltas pass through UNSCALED,
    /// matching the reference iced_winit conversion. Dividing by the scale
    /// factor halves trackpad speed on Retina (the settings Input tab —
    /// the only consistently-overflowing native scroll view — feels dead).
    #[test]
    fn wheel_pixel_deltas_pass_through_unscaled() {
        use tao::event::MouseScrollDelta;
        let mut cursor = mouse::Cursor::Unavailable;
        let mut modifiers = k::Modifiers::empty();
        let mut wheel = |delta| {
            convert(
                &WindowEvent::MouseWheel {
                    device_id: unsafe { tao::event::DeviceId::dummy() },
                    delta,
                    phase: tao::event::TouchPhase::Moved,
                    modifiers: tk::ModifiersState::empty(),
                },
                2.0,
                Point::ORIGIN,
                &mut cursor,
                &mut modifiers,
            )
        };
        assert_eq!(
            wheel(MouseScrollDelta::LineDelta(0.0, 1.0)),
            Some(Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }))
        );
        // Retina scale must not shrink relative motion: 20 physical units in
        // means 20 iced pixels out, exactly like iced_winit.
        assert_eq!(
            wheel(MouseScrollDelta::PixelDelta(
                tao::dpi::PhysicalPosition::new(0.0, 20.0)
            )),
            Some(Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y: 20.0 },
            }))
        );
    }

    /// Coalesced wheel accumulation: same units add, mixed units refuse so
    /// the caller flushes first and unit semantics never corrupt.
    #[test]
    fn wheel_deltas_coalesce_within_variant_only() {
        assert_eq!(
            merge_wheel(None, mouse::ScrollDelta::Pixels { x: 0.0, y: 4.0 }),
            Some(mouse::ScrollDelta::Pixels { x: 0.0, y: 4.0 })
        );
        assert_eq!(
            merge_wheel(
                Some(mouse::ScrollDelta::Pixels { x: 1.0, y: 4.0 }),
                mouse::ScrollDelta::Pixels { x: -2.0, y: 8.0 }
            ),
            Some(mouse::ScrollDelta::Pixels { x: -1.0, y: 12.0 })
        );
        assert_eq!(
            merge_wheel(
                Some(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
                mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }
            ),
            Some(mouse::ScrollDelta::Lines { x: 0.0, y: 0.0 })
        );
        assert_eq!(
            merge_wheel(
                Some(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
                mouse::ScrollDelta::Pixels { x: 0.0, y: 1.0 }
            ),
            None
        );
    }

    /// Only dirty pumps schedule a redraw: a no-op wheel tick or a clean
    /// hover must not cost a full rebuild+present, while any redraw
    /// request, outdated tree, or pending message must.
    #[test]
    fn clean_pumps_need_no_redraw() {
        use iced_winit::{core::window::RedrawRequest, runtime::user_interface::State};
        let updated = |redraw_request| State::Updated {
            mouse_interaction: mouse::Interaction::default(),
            redraw_request,
            input_method: iced::advanced::input_method::InputMethod::Disabled,
            has_layout_changed: false,
        };
        assert!(!needs_redraw(&updated(RedrawRequest::Wait), false));
        assert!(needs_redraw(&updated(RedrawRequest::Wait), true));
        assert!(needs_redraw(&updated(RedrawRequest::NextFrame), false));
        assert!(needs_redraw(
            &updated(RedrawRequest::At(iced::time::Instant::now())),
            false
        ));
        assert!(needs_redraw(&State::Outdated, false));
    }
}
