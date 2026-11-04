//! Native loop adapted from prototype_1; rendering and futures remain UI-local.
use super::{Message, TaoHost, View, input, panic_error, scheduler::Scheduler};
use iced::{Size, mouse};
#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
use iced_tiny_skia::{
    graphics::Compositor as _,
    window::{Compositor, Surface, compositor},
};
use iced_winit::{
    graphics::Viewport,
    runtime::user_interface::{State, UserInterface},
};
use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::{Rc, Weak},
    sync::{Arc, Mutex, Once},
    time::Instant,
};
use tao::{
    event::{ElementState as NativeElementState, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::run_return::EventLoopExtRunReturn,
    window::{Window, WindowBuilder},
};
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
use {gtk::prelude::*, tao::platform::unix::WindowExtUnix};

#[cfg(all(
    test,
    any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )
))]
mod input_tests;

#[derive(Debug)]
enum Native {
    Wake(usize),
    Refresh(usize),
    Turn,
    Menu(muda::MenuId),
}
#[derive(Clone)]
struct Proxy(EventLoopProxy<Native>);
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
static NUDGE_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
impl Proxy {
    fn send(&self, event: Native) {
        let sent = self.0.send_event(event).is_ok();
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        )))]
        let _ = sent;
        // Tao Linux's crossbeam proxy does not wake blocking gtk_main_iteration_do.
        // This is a one-shot wake, not a periodic poll or a cross-thread UI call.
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        if sent && !NUDGE_PENDING.swap(true, std::sync::atomic::Ordering::AcqRel) {
            gtk::glib::idle_add_once(|| {
                NUDGE_PENDING.store(false, std::sync::atomic::Ordering::Release);
            });
        }
    }
}
// muda's process-global OnceLock handler cannot be cleared with None.
static MENU_PROXY: Mutex<Option<Proxy>> = Mutex::new(None);
static MENU_INIT: Once = Once::new();

// Automatic window tabbing is a process-wide AppKit default (tao exposes it
// per builder via `WindowBuilderExtMacOS`, but `Desktop::open` builds the
// window internally). Panechord defaults to opted out, so separate windows
// never merge into tabs unless the app opts in before `run_host`.
static ALLOW_AUTOMATIC_WINDOW_TABBING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Opt out of (or back into) AppKit automatic window tabbing. Takes effect
/// for windows created after the call; off macOS it only records the flag.
pub(crate) fn set_allows_automatic_window_tabbing(enabled: bool) {
    ALLOW_AUTOMATIC_WINDOW_TABBING.store(enabled, std::sync::atomic::Ordering::SeqCst);
}

// Remembered main-window size (logical points) consumed once by the next
// built window; falls back to the default when unset or invalid.
static INITIAL_WINDOW_SIZE: std::sync::Mutex<Option<(f64, f64)>> = std::sync::Mutex::new(None);

/// Stage the initial main-window size, in logical points. Consumed once by
/// the next built window; call before `run_host` builds views.
pub(crate) fn set_initial_window_size(width: u32, height: u32) {
    if width >= 100 && height >= 100 && width <= 8192 && height <= 8192 {
        *INITIAL_WINDOW_SIZE.lock().unwrap() = Some((f64::from(width), f64::from(height)));
    }
}

fn take_initial_window_size() -> (f64, f64) {
    INITIAL_WINDOW_SIZE
        .lock()
        .unwrap()
        .take()
        .unwrap_or((660.0, 520.0))
}
struct MenuBridge;
impl Drop for MenuBridge {
    fn drop(&mut self) {
        MENU_PROXY.lock().unwrap().take();
    }
}
struct ExecutorGuard(Rc<Scheduler>);
impl Drop for ExecutorGuard {
    fn drop(&mut self) {
        self.0.close();
    }
}

// iced_winit::Clipboard::connect requires Arc<winit::Window>, not a Tao window.
// Use its identical lower layer, retaining the native display owner until drop.
struct Clipboard {
    native: Option<window_clipboard::Clipboard>,
    _window: Arc<Window>,
}
impl Clipboard {
    fn connect(window: Arc<Window>, view: &View) -> Self {
        // SAFETY: _window outlives native, including its display handle.
        let native = match unsafe { window_clipboard::Clipboard::connect(window.as_ref()) } {
            Ok(native) => Some(native),
            Err(error) => {
                view.0
                    .diagnostics
                    .borrow_mut()
                    .push(format!("system clipboard unavailable: {error}"));
                None
            }
        };
        Self {
            native,
            _window: window,
        }
    }
}
impl iced::advanced::Clipboard for Clipboard {
    fn read(&self, kind: iced::advanced::clipboard::Kind) -> Option<String> {
        let clipboard = self.native.as_ref()?;
        match kind {
            iced::advanced::clipboard::Kind::Standard => clipboard.read().ok(),
            iced::advanced::clipboard::Kind::Primary => {
                clipboard.read_primary().and_then(Result::ok)
            }
        }
    }
    fn write(&mut self, kind: iced::advanced::clipboard::Kind, value: String) {
        let Some(clipboard) = self.native.as_mut() else {
            return;
        };
        let result = match kind {
            iced::advanced::clipboard::Kind::Standard => clipboard.write(value),
            iced::advanced::clipboard::Kind::Primary => {
                clipboard.write_primary(value).unwrap_or(Ok(()))
            }
        };
        if let Err(error) = result {
            eprintln!("panechord-tao clipboard write: {error}");
        }
    }
}
fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// Rebuild the retained iced tree iff this is the first pump or bindings /
/// structure moved (`Inner::revision`). Bounds-only changes relayout the
/// existing root; steady input events (cursor, wheel, hover) reuse the tree
/// and only `update`. Reconstructing every widget per tick is what made
/// the Input tab (all rows mounted) slow down even the untouched left tab.
fn needs_rebuild(ui_none: bool, revision: u64, ui_revision: u64) -> bool {
    ui_none || revision != ui_revision
}

/// Mounted and visible through ancestors (enabled controls still display,
// so enablement is ignored): images failing this impose no window floor.
fn displayed(node: &super::Node) -> bool {
    if !node.mounted.get() || !node.visible.get() {
        return false;
    }
    let mut current = node.parent.upgrade();
    while let Some(ancestor) = current {
        if !ancestor.visible.get() {
            return false;
        }
        current = ancestor.parent.upgrade();
    }
    true
}
/// Whether a pump rasterizes after updating state.
// Variants split by platform: Linux frames use `IfDirty`, other targets
// use `Always` for OS-requested exposes, so one side is always unused.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Paint {
    /// State-only pump: scroll flushes and input-event evaluations.
    Never,
    /// Frame pump that paints only when the update dirtied anything, so
    /// coalesced no-op scrolls skip the screenshot and cairo present.
    IfDirty,
    /// Frame pump that always paints (OS-requested redraws).
    Always,
}
/// What a pump did: the iced messages plus whether anything dirtied (a
/// frame should be scheduled) and whether it rasterized (pixels are fresh
/// and worth presenting). They differ on `IfDirty` first frames/resizes,
/// which paint without iced reporting dirty.
#[derive(Default)]
struct Pumped {
    messages: Vec<Message>,
    dirty: bool,
    painted: bool,
}
struct Painter {
    window: Arc<Window>,
    renderer: iced_tiny_skia::Renderer,
    /// Retained iced tree: rebuilt only when bindings/structure move
    /// (`Inner::revision`) or on the first pump; pure input events
    /// (cursor, wheel, hover) reuse it via `update` instead of
    /// reconstructing every widget per tick. Previously every pump ran
    /// `root.element()` + `UserInterface::build` over the whole tree, so
    /// the Input tab (every binding row mounted) paid a full rebuild for
    /// events landing on the untouched left tab as well.
    ui: Option<UserInterface<'static, Message, iced::Theme, iced_tiny_skia::Renderer>>,
    ui_revision: u64,
    ui_bounds: Cell<Option<(u32, u32)>>,
    clipboard: Clipboard,
    cursor: mouse::Cursor,
    modifiers: iced::keyboard::Modifiers,
    deadline: Option<Instant>,
    ime_cursor: Option<iced::Point>,
    media_applied: Cell<u64>,
    /// Coalesced wheel scroll awaiting delivery. Trackpads emit dozens of
    /// ticks per second, so same-unit ticks from every
    /// source accumulate here and flush once per frame instead of
    /// updating per tick.
    pending_wheel: Cell<Option<mouse::ScrollDelta>>,
    /// Sticky paint debt: state-only pumps set it when they dirty iced
    /// internals, and only an actual paint clears it (see `pump`).
    tainted: Cell<bool>,
    /// Last painted logical size: first frames and resizes paint even when
    /// iced reports clean (see the `IfDirty` branch in `pump`).
    painted_size: Cell<Option<(u32, u32)>>,
}
impl Painter {
    fn new(window: Arc<Window>, view: &View) -> Self {
        Self {
            renderer: iced_tiny_skia::Renderer::new(iced::Font::default(), iced::Pixels(16.0)),
            ui: None,
            ui_revision: 0,
            ui_bounds: Cell::new(None),
            clipboard: Clipboard::connect(window.clone(), view),
            cursor: mouse::Cursor::Unavailable,
            modifiers: Default::default(),
            deadline: None,
            ime_cursor: None,
            window,
            media_applied: Cell::new(0),
            pending_wheel: Cell::new(None),
            tainted: Cell::new(false),
            painted_size: Cell::new(None),
        }
    }
    /// Returns the pumped messages, whether anything dirtied, and whether it
    /// painted. State-only pumps (`Never`) only feed iced: a scroll offset
    /// or caret may move inside them without touching pixels, so their dirty
    /// accrues as sticky paint debt the next frame must honor. Frame pumps
    /// paint (`Always` for OS-requested exposes, `IfDirty` for coalesced
    /// scroll frames so no-op ticks skip the screenshot).
    fn pump(&mut self, view: &View, bounds: Size, events: &[iced::Event], paint: Paint) -> Pumped {
        use iced_winit::core::window::RedrawRequest;
        view.0.surface_bounds.set((bounds.width, bounds.height));
        let viewport = view.0.native_viewport.borrow().clone();
        if let Some(viewport) = viewport {
            viewport(bounds.width, bounds.height);
        }
        if events.iter().any(|event| {
            matches!(
                event,
                iced::Event::Window(iced::window::Event::RedrawRequested(_))
            )
        }) {
            self.deadline = None;
        }
        let revision = view.0.revision.get();
        let size_bits = (bounds.width.to_bits(), bounds.height.to_bits());
        // Window roots do NOT scroll: the root gets the bounded viewport
        // height directly so Tab bodies (Fill when bounded) take the
        // remainder and footers stay pinned. Tall pages scroll inside via
        // their own Scroll (input, notifications) or ItemsControl viewport
        // (system). Wrapping the whole window in a scrollable offers
        // unbounded height to the root, sizing tall content to full height
        // and pushing the footer out of view (see
        // bounded_window_root_pins_footer_while_tall_page_scrolls_inside).
        // Dialogs stay overlaid outside via scene().
        //
        // Retained tree: reconstruct the Element only when bindings or
        // structure moved (revision) or on the first pump. Bounds-only
        // changes relayout the existing root; steady input events reuse
        // the tree and only `update` below.
        let structure_changed = self.ui.is_none() || revision != self.ui_revision;
        if structure_changed {
            let root = &view.0.root.0;
            let content = root.element();
            let cache = self.ui.take().map(|ui| ui.into_cache()).unwrap_or_default();
            let built =
                UserInterface::build(view.scene(content), bounds, cache, &mut self.renderer);
            self.ui = Some(built);
            self.ui_revision = revision;
            self.ui_bounds.set(Some(size_bits));
        } else if self.ui_bounds.get() != Some(size_bits) {
            let retained = self.ui.take().expect("retained iced tree");
            self.ui = Some(retained.relayout(bounds, &mut self.renderer));
            self.ui_bounds.set(Some(size_bits));
        }
        // Image floor: narrowing must keep source pixels instead of
        // squeezing them (spec rule 4), matching the other backends whose
        // toolkits derive window minima from content. Recompute only when
        // image sizes change, then clamp the OS window minimum.
        if view.0.media_version.get() != self.media_applied.get() {
            self.media_applied.set(view.0.media_version.get());
            let mut floor = (0u32, 0u32);
            for node in view.0.nodes.borrow().values().filter_map(Weak::upgrade) {
                let (width, height) = node.media.get();
                if (width, height) == (0, 0) || !displayed(&node) {
                    continue;
                }
                let (mut pad_w, mut pad_h) = (0.0f32, 0.0f32);
                let mut current = node.parent.upgrade();
                while let Some(ancestor) = current {
                    let style = ancestor.style.get();
                    pad_w += finite(style.margin.left)
                        + finite(style.margin.right)
                        + finite(style.padding.left)
                        + finite(style.padding.right);
                    pad_h += finite(style.margin.top)
                        + finite(style.margin.bottom)
                        + finite(style.padding.top)
                        + finite(style.padding.bottom);
                    current = ancestor.parent.upgrade();
                }
                floor.0 = floor.0.max(width.saturating_add(pad_w.ceil() as u32));
                floor.1 = floor.1.max(height.saturating_add(pad_h.ceil() as u32));
            }
            self.window.set_min_inner_size(if floor == (0, 0) {
                None
            } else {
                Some(tao::dpi::LogicalSize::new(
                    f64::from(floor.0),
                    f64::from(floor.1),
                ))
            });
        }
        // Automation/bound viewport changes scroll the real iced widget, not just row data.
        if let Some(ui) = self.ui.as_mut() {
            for (id, offset) in view.0.scroll.take() {
                use iced::advanced::widget::{Id, operation::scrollable};
                let mut operation = scrollable::scroll_to::<()>(
                    Id::from(id),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(offset),
                    },
                );
                ui.operate(&self.renderer, &mut operation);
            }
        }
        let mut messages = Vec::new();
        // iced text inputs retain their own modifier state. Clearing only the
        // Tao converter on focus loss leaves Ctrl/Shift stuck if key-up goes to
        // another window. Reset widgets before delivering the unfocus event.
        let with_reset;
        let events: &[iced::Event] = if matches!(
            events,
            [iced::Event::Window(iced::window::Event::Unfocused)]
        ) {
            with_reset = [
                iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                    iced::keyboard::Modifiers::empty(),
                )),
                events[0].clone(),
            ];
            &with_reset
        } else {
            events
        };
        let ui = self.ui.as_mut().expect("retained iced tree");
        let (state, _) = ui.update(
            events,
            self.cursor,
            &mut self.renderer,
            &mut self.clipboard,
            &mut messages,
        );
        let dirty = input::needs_redraw(&state, !messages.is_empty());
        // Sticky paint debt: a state-only pump may move iced-internal state
        // (scroll offsets, carets) that later pumps see as quiescent. The
        // next frame must still present it; any actual paint clears the debt.
        if paint == Paint::Never {
            self.tainted.set(self.tainted.get() || dirty);
        }
        if let State::Updated { input_method, .. } = &state {
            self.ime_cursor = match input_method {
                iced::advanced::input_method::InputMethod::Enabled { cursor, .. } => {
                    Some(iced::Point::new(cursor.x, cursor.y + cursor.height))
                }
                _ => None,
            };
        }
        match state {
            State::Updated {
                redraw_request: RedrawRequest::At(at),
                ..
            } => {
                self.deadline = Some(self.deadline.map_or(at, |old| old.min(at)));
            }
            State::Updated {
                redraw_request: RedrawRequest::NextFrame,
                ..
            }
            | State::Outdated => self.deadline = Some(Instant::now()),
            _ => {}
        }
        if match paint {
            Paint::Always => true,
            Paint::Never => false,
            Paint::IfDirty => {
                dirty || self.tainted.get() || {
                    // First frame and resizes must paint even when iced
                    // reports clean: a bare RedrawRequested yields Wait
                    // although no pixels were ever presented (or the surface
                    // was reconfigured for a new size).
                    let size = (bounds.width.to_bits(), bounds.height.to_bits());
                    self.painted_size.replace(Some(size)) != Some(size)
                }
            }
        } {
            let ui = self.ui.as_mut().expect("retained iced tree");
            ui.draw(
                &mut self.renderer,
                &iced::Theme::Light,
                &iced::advanced::renderer::Style::default(),
                self.cursor,
            );
            view.0.frames.set(view.0.frames.get() + 1);
            self.tainted.set(false);
            return Pumped {
                messages,
                dirty,
                painted: true,
            };
        }
        Pumped {
            messages,
            dirty,
            painted: false,
        }
    }
}
fn dispatch(view: &View, messages: Vec<Message>) {
    for message in messages {
        let _ = view.dispatch(message);
        if view.0.lifecycle.is_closed() {
            break;
        }
    }
}
fn fail(view: &View, error: String) {
    view.0.fatal_error.replace(Some(error.clone()));
    view.0.report(error);
    view.close();
}
struct Desktop {
    window: Arc<Window>,
    view: View,
    painter: Rc<RefCell<Painter>>,
    active: std::cell::Cell<bool>,
    // Last applied window chrome: `set_visible(true)` re-keys the window on
    // macOS (`make_key_and_order_front`), so reapplying it from unfocused or
    // deadline paths steals focus into a ping-pong. Only transition.
    shown: std::cell::Cell<bool>,
    titled: RefCell<String>,
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))]
    area: gtk::DrawingArea,
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))]
    signals: Vec<(gtk::Widget, gtk::glib::SignalHandlerId)>,
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )))]
    compositor: Compositor,
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )))]
    surface: Surface,
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )))]
    size: (u32, u32),
}
impl Desktop {
    fn open(
        target: &tao::event_loop::EventLoopWindowTarget<Native>,
        view: &View,
        proxy: Proxy,
    ) -> Result<Self, String> {
        let (initial_width, initial_height) = take_initial_window_size();
        // `mut` is only consumed by the macOS tabbing hook below; silence
        // the Linux unused-mut lint without touching that path.
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
        let mut builder = WindowBuilder::new()
            .with_title(view.title())
            .with_visible(false)
            .with_inner_size(tao::dpi::LogicalSize::new(initial_width, initial_height));
        #[cfg(target_os = "macos")]
        {
            use tao::platform::macos::WindowBuilderExtMacOS;
            builder = builder.with_automatic_window_tabbing(
                ALLOW_AUTOMATIC_WINDOW_TABBING.load(std::sync::atomic::Ordering::SeqCst),
            );
        }
        let window = Arc::new(builder.build(target).map_err(|e| e.to_string())?);
        let painter = Rc::new(RefCell::new(Painter::new(window.clone(), view)));
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        {
            let vbox = window
                .default_vbox()
                .ok_or("Tao GTK3 window has no default vbox")?;
            view.0
                .menus()
                .init_for_gtk_window(window.gtk_window(), Some(vbox))
                .map_err(|e| e.to_string())?;
            let area = gtk::DrawingArea::new();
            area.set_has_window(false);
            area.set_can_focus(true);
            area.set_hexpand(true);
            area.set_vexpand(true);
            vbox.pack_end(&area, true, true, 0);
            let out = painter.clone();
            let owner = view.clone();
            let draw = area.connect_draw(move |area, context| {
                if owner.0.lifecycle.is_closed() {
                    return gtk::glib::Propagation::Stop;
                }
                let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                    let scale = f64::from(area.scale_factor());
                    let (width, height) = (area.allocated_width(), area.allocated_height());
                    if width <= 0 || height <= 0 {
                        return Ok(());
                    }
                    let viewport = Viewport::with_physical_size(
                        Size::new(
                            (f64::from(width) * scale) as u32,
                            (f64::from(height) * scale) as u32,
                        ),
                        scale as f32,
                    );
                    let outcome = {
                        let mut painter = out.borrow_mut();
                        let bounds = viewport.logical_size();
                        // One frame carries the whole coalesced scroll in a
                        // single update: the bridge and the tao wheel branch
                        // only accumulate, so N ticks cost one update, not N.
                        // Atomicity matters — iced reports movement inside the
                        // same update that requests its paint; a split flush
                        // would look quiescent to the paint decision.
                        let mut events = Vec::with_capacity(2);
                        if let Some(delta) = painter.pending_wheel.take() {
                            events.push(input::wheel_event(delta));
                        }
                        events.push(redraw());
                        let redrawn = painter.pump(&owner, bounds, &events, Paint::IfDirty);
                        let messages = redrawn.messages;
                        if !redrawn.painted {
                            (None, messages)
                        } else {
                            let pixels = iced_tiny_skia::window::compositor::screenshot(
                                &mut painter.renderer,
                                &viewport,
                                iced::Color::WHITE,
                            );
                            (Some(pixels), messages)
                        }
                    };
                    let (pixels, messages) = outcome;
                    let Some(mut pixels) = pixels else {
                        dispatch(&owner, messages);
                        return Ok(());
                    };
                    for pixel in pixels.as_chunks_mut::<4>().0 {
                        let native =
                            u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]]).to_ne_bytes();
                        pixel.copy_from_slice(&native);
                    }
                    let size = viewport.physical_size();
                    let image = gtk::cairo::ImageSurface::create_for_data(
                        pixels,
                        gtk::cairo::Format::Rgb24,
                        size.width as i32,
                        size.height as i32,
                        (size.width * 4) as i32,
                    )
                    .map_err(|e| e.to_string())?;
                    image.set_device_scale(scale, scale);
                    context
                        .set_source_surface(&image, 0.0, 0.0)
                        .map_err(|e| e.to_string())?;
                    context.paint().map_err(|e| e.to_string())?;
                    if std::env::var_os("PANECHORD_CONTROLS_REVIEW").is_some() {
                        eprintln!("CONTROLS3 DRAW {width}x{height}");
                    }
                    dispatch(&owner, messages);
                    Ok(())
                }));
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => fail(&owner, e),
                    Err(p) => fail(&owner, panic_error(p)),
                }
                proxy.send(Native::Turn);
                gtk::glib::Propagation::Stop
            });
            let out = Rc::downgrade(&painter);
            let owner = view.downgrade();
            let surface = area.downgrade();
            let scroll = window
                .gtk_window()
                .connect_scroll_event(move |window, event| {
                    // Discrete wheel clicks and smooth (touchpad and
                    // high-resolution wheel) deltas share the iced mapping
                    // through `input::scroll_delta`. Anything else (e.g. an
                    // unknown direction) is not ours: let GTK have it.
                    // Smooth must be handled: dropping it silently kills
                    // touchpad scrolling, the dominant scroll source on
                    // modern Linux (settings Input tab feels dead).
                    let kind = match event.direction() {
                        gtk::gdk::ScrollDirection::Up => input::NativeScroll::Up,
                        gtk::gdk::ScrollDirection::Down => input::NativeScroll::Down,
                        gtk::gdk::ScrollDirection::Left => input::NativeScroll::Left,
                        gtk::gdk::ScrollDirection::Right => input::NativeScroll::Right,
                        gtk::gdk::ScrollDirection::Smooth => {
                            let (x, y) = event.delta();
                            input::NativeScroll::Smooth(x, y)
                        }
                        _ => return gtk::glib::Propagation::Proceed,
                    };
                    let delta = input::scroll_delta(kind);
                    if let (Some(painter), Some(view), Some(area)) =
                        (out.upgrade(), owner.upgrade(), surface.upgrade())
                    {
                        let offset = area.translate_coordinates(window, 0, 0).unwrap_or_default();
                        let position = event.position();
                        let cursor = iced::Point::new(
                            position.0 as f32 - offset.0 as f32,
                            position.1 as f32 - offset.1 as f32,
                        );
                        if cursor.x >= 0.0 && cursor.y >= 0.0 {
                            {
                                let mut painter = painter.borrow_mut();
                                painter.cursor = mouse::Cursor::Available(cursor);
                                // The bridge owns Linux scroll: merge into the
                                // coalesced pending total instead of pumping
                                // per tick. Mixed units flush first so order
                                // — and unit semantics — never corrupt.
                                if input::merge_wheel(painter.pending_wheel.get(), delta).is_none()
                                {
                                    if let Some(previous) = painter.pending_wheel.take() {
                                        let wheel = input::wheel_event(previous);
                                        let flushed = painter.pump(
                                            &view,
                                            Size::new(
                                                area.allocated_width() as f32,
                                                area.allocated_height() as f32,
                                            ),
                                            std::slice::from_ref(&wheel),
                                            Paint::Never,
                                        );
                                        dispatch(&view, flushed.messages);
                                    }
                                    painter.pending_wheel.set(Some(delta));
                                } else {
                                    painter.pending_wheel.set(input::merge_wheel(
                                        painter.pending_wheel.get(),
                                        delta,
                                    ));
                                }
                            }
                            // The frame decision moves to draw: it flushes the
                            // pending total and paints only when dirty, so a
                            // no-op tick costs no screenshot.
                            area.queue_draw();
                        }
                    }
                    gtk::glib::Propagation::Proceed
                });
            area.show();
            let signals = vec![
                (area.clone().upcast(), draw),
                (window.gtk_window().clone().upcast(), scroll),
            ];
            Ok(Self {
                window,
                view: view.clone(),
                painter,
                active: std::cell::Cell::new(false),
                shown: std::cell::Cell::new(false),
                titled: RefCell::new(String::new()),
                area,
                signals,
            })
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        )))]
        {
            let _ = proxy;
            #[cfg(target_os = "windows")]
            {
                use tao::platform::windows::WindowExtWindows;
                unsafe { view.0.menus().init_for_hwnd(window.hwnd() as isize) }
                    .map_err(|e| e.to_string())?;
            }
            let mut compositor =
                compositor::new(iced_tiny_skia::Settings::default(), window.clone());
            let size = window.inner_size();
            let surface = compositor.create_surface(window.clone(), size.width, size.height);
            Ok(Self {
                window,
                view: view.clone(),
                painter,
                active: std::cell::Cell::new(false),
                shown: std::cell::Cell::new(false),
                titled: RefCell::new(String::new()),
                compositor,
                surface,
                size: (size.width, size.height),
            })
        }
    }
    fn bounds(&self) -> Size {
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        {
            Size::new(
                self.area.allocated_width().max(0) as f32,
                self.area.allocated_height().max(0) as f32,
            )
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        )))]
        {
            let s = self
                .window
                .inner_size()
                .to_logical::<f64>(self.window.scale_factor());
            Size::new(s.width as f32, s.height as f32)
        }
    }
    fn activate(&self) {
        self.active.set(true);
        #[cfg(target_os = "macos")]
        self.view.0.menus().init_for_nsapp();
    }
    fn offset(&self) -> iced::Point {
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        {
            let (x, y) = self
                .area
                .translate_coordinates(self.window.gtk_window(), 0, 0)
                .unwrap_or_default();
            iced::Point::new(x as f32, y as f32)
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        )))]
        {
            iced::Point::ORIGIN
        }
    }
    fn refresh(&self) {
        // Transition-only chrome: reasserting visibility re-keys the window
        // on macOS and bounces focus between windows; re-setting an
        // unchanged title is wasted IPC on every event.
        let title = self.view.title();
        if self.titled.borrow().as_str() != title {
            self.window.set_title(&title);
            self.titled.replace(title);
        }
        let shown = self.view.0.visible.get() && !self.view.0.lifecycle.is_closed();
        if self.shown.get() != shown {
            self.window.set_visible(shown);
            self.shown.set(shown);
        }
        if let Some(cursor) = self.painter.borrow().ime_cursor {
            let offset = self.offset();
            self.window.set_ime_position(tao::dpi::LogicalPosition::new(
                cursor.x + offset.x,
                cursor.y + offset.y,
            ));
        }
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        self.area.queue_draw();
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        )))]
        self.window.request_redraw();
    }
    /// Deliver a coalesced wheel scroll before anything ordered after it.
    /// Returns whether the flush dirtied anything (the caller folds it
    /// into its own redraw decision so a real scroll is never dropped).
    fn flush_wheel(&self) -> bool {
        let bounds = self.bounds();
        let mut painter = self.painter.borrow_mut();
        let Some(delta) = painter.pending_wheel.take() else {
            return false;
        };
        let wheel = input::wheel_event(delta);
        let pumped = painter.pump(
            &self.view,
            bounds,
            std::slice::from_ref(&wheel),
            Paint::Never,
        );
        dispatch(&self.view, pumped.messages);
        pumped.dirty
    }
    fn event(&self, event: &WindowEvent<'_>) {
        // Linux scroll ownership: the GTK bridge above owns wheel input
        // (precise cursor, coalesced frames). Tao delivers the same physical
        // ticks as WindowEvent::MouseWheel; handling both would double-apply
        // them and strand duplicates in pending_wheel, which Linux draws
        // never flushed. Drop the tao duplicates.
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        if matches!(event, WindowEvent::MouseWheel { .. }) {
            return;
        }
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        if self.painter.borrow().cursor.position().is_some()
            && matches!(
                event,
                WindowEvent::MouseInput {
                    state: tao::event::ElementState::Pressed,
                    ..
                }
            )
        {
            self.area.grab_focus();
        }
        // High-frequency wheel ticks accumulate instead of pumping a full
        // tree rebuild per tick: the pending total flushes once per frame
        // (in `draw`, or here before any event ordered after it).
        if matches!(event, WindowEvent::MouseWheel { .. }) {
            let scale = self.window.scale_factor();
            let offset = self.offset();
            let bounds = self.bounds();
            let mut painter = self.painter.borrow_mut();
            let Painter {
                cursor, modifiers, ..
            } = &mut *painter;
            if let Some(iced::Event::Mouse(mouse::Event::WheelScrolled { delta })) =
                input::convert(event, scale, offset, cursor, modifiers)
            {
                if input::merge_wheel(painter.pending_wheel.get(), delta).is_none() {
                    // Mixed units (trackpad Pixels vs wheel Lines): deliver
                    // what's pending first so order — and units — survive.
                    if let Some(previous) = painter.pending_wheel.take() {
                        let wheel = input::wheel_event(previous);
                        let flushed = painter.pump(
                            &self.view,
                            bounds,
                            std::slice::from_ref(&wheel),
                            Paint::Never,
                        );
                        dispatch(&self.view, flushed.messages);
                    }
                    painter.pending_wheel.set(Some(delta));
                } else {
                    painter
                        .pending_wheel
                        .set(input::merge_wheel(painter.pending_wheel.get(), delta));
                }
                drop(painter);
                self.refresh();
                return;
            }
            // Unknown scroll variant maps to no iced event: nothing to pump
            // and nothing dirtied, so no redraw.
            return;
        }
        // A non-wheel event belongs after any pending scroll.
        let flushed = self.flush_wheel();
        let pumped = self.pump_event(event);
        dispatch(&self.view, pumped.messages);
        if flushed || pumped.dirty {
            self.refresh();
        }
    }
    /// Single-event pump shared by the wheel / non-wheel branches above:
    /// dialog keys first, otherwise the converted native event.
    fn pump_event(&self, event: &WindowEvent<'_>) -> Pumped {
        let mut painter = self.painter.borrow_mut();
        let Painter {
            cursor, modifiers, ..
        } = &mut *painter;
        // HIG dialog keys: plain Return/Escape inside an open dialog
        // clicks the footer before widgets see the press. Entries keep
        // Return when no dialog is open, and IME composition keys never
        // arrive here as presses.
        if let WindowEvent::KeyboardInput { event, .. } = event
            && event.state == NativeElementState::Pressed
            && !event.repeat
            && modifiers.is_empty()
            && let code = event.physical_key
            && let Some(message) = self.view.dialog_key_action(code)
        {
            Pumped {
                messages: vec![message],
                dirty: true,
                painted: false,
            }
        } else {
            let mapped = input::convert(
                event,
                self.window.scale_factor(),
                self.offset(),
                cursor,
                modifiers,
            );
            mapped
                .map(|e| {
                    painter.pump(
                        &self.view,
                        self.bounds(),
                        std::slice::from_ref(&e),
                        Paint::Never,
                    )
                })
                .unwrap_or_default()
        }
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )))]
    fn draw(&mut self) -> Result<(), String> {
        let s = self.window.inner_size();
        if s.width == 0 || s.height == 0 {
            return Ok(());
        }
        if self.size != (s.width, s.height) {
            self.compositor
                .configure_surface(&mut self.surface, s.width, s.height);
            self.size = (s.width, s.height);
        }
        let viewport = Viewport::with_physical_size(
            Size::new(s.width, s.height),
            self.window.scale_factor() as f32,
        );
        let messages = {
            let mut painter = self.painter.borrow_mut();
            // One frame carries the whole coalesced scroll in a single
            // update, so N ticks cost one update, not N.
            let mut events = Vec::with_capacity(2);
            if let Some(delta) = painter.pending_wheel.take() {
                events.push(input::wheel_event(delta));
            }
            events.push(redraw());
            let redrawn = painter.pump(&self.view, viewport.logical_size(), &events, Paint::Always);
            let messages = redrawn.messages;
            self.compositor
                .present(
                    &mut painter.renderer,
                    &mut self.surface,
                    &viewport,
                    iced::Color::WHITE,
                    || {},
                )
                .map_err(|e| e.to_string())?;
            messages
        };
        dispatch(&self.view, messages);
        Ok(())
    }
}
impl Drop for Desktop {
    fn drop(&mut self) {
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        {
            for (widget, signal) in self.signals.drain(..) {
                widget.disconnect(signal);
            }
            if let Err(error) = super::menu::clear_accelerators(&self.view.0.menus().items()) {
                self.view.0.report(error);
            }
            let _ = self
                .view
                .0
                .menus()
                .remove_for_gtk_window(self.window.gtk_window());
            unsafe {
                self.area.destroy();
            }
        }
        #[cfg(target_os = "windows")]
        {
            use tao::platform::windows::WindowExtWindows;
            unsafe {
                let _ = self
                    .view
                    .0
                    .menus()
                    .remove_for_hwnd(self.window.hwnd() as isize);
            }
        }
        #[cfg(target_os = "macos")]
        if self.active.get() {
            self.view.0.menus().remove_for_nsapp();
        }
    }
}
fn redraw() -> iced::Event {
    iced::Event::Window(iced::window::Event::RedrawRequested(Instant::now()))
}
pub(super) fn run(
    make: impl FnOnce(Rc<TaoHost>) -> Result<(), String> + 'static,
) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        let mut builder = EventLoopBuilder::<Native>::with_user_event();
        #[cfg(target_os = "windows")]
        let menu_owner = {
            use tao::platform::windows::EventLoopBuilderExtWindows;
            use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, TranslateAcceleratorW};
            let owner = Rc::new(RefCell::new(std::rc::Weak::<super::Inner>::new()));
            let hook = owner.clone();
            builder.with_msg_hook(move |message| {
                let inner = hook.borrow().upgrade();
                let Some(inner) = inner.filter(|i| !i.lifecycle.is_closed()) else {
                    return false;
                };
                // Query each time: dynamic menu snapshots may replace the HACCEL.
                let accelerator = inner.menus().haccel();
                if message.is_null() || accelerator == 0 {
                    return false;
                }
                // SAFETY: Tao provides a valid MSG for this synchronous hook; the
                // live View owns the menu table throughout TranslateAcceleratorW.
                unsafe {
                    let message = message.cast::<MSG>();
                    TranslateAcceleratorW((*message).hwnd, accelerator as _, message) != 0
                }
            });
            owner
        };
        let mut event_loop = builder.build();
        let proxy = Proxy(event_loop.create_proxy());
        let wake = proxy.clone();
        let executor = Rc::new(Scheduler::new(Arc::new(move |id| {
            wake.send(Native::Wake(id))
        })));
        let _executor_guard = ExecutorGuard(executor.clone());
        let invalidate = proxy.clone();
        let host = Rc::new(TaoHost {
            executor: executor.clone(),
            views: RefCell::default(),
            session: RefCell::default(),
            next: std::cell::Cell::new(0),
            notify: Rc::new(move |id| invalidate.send(Native::Refresh(id))),
        });
        make(host.clone())?;
        let mut desktops = std::collections::BTreeMap::<usize, Desktop>::new();
        let mut active = None;
        let mut error = None;
        MENU_PROXY.lock().unwrap().replace(proxy.clone());
        let _bridge = MenuBridge;
        MENU_INIT.call_once(|| {
            muda::MenuEvent::set_event_handler(Some(|event: muda::MenuEvent| {
                let proxy = MENU_PROXY.lock().unwrap().clone();
                if let Some(proxy) = proxy {
                    proxy.send(Native::Menu(event.id));
                }
            }))
        });
        let status = event_loop.run_return(|event, target, flow| {
            let pending = host.views.borrow().clone();
            for (id, view) in pending {
                if view.0.lifecycle.is_closed() || desktops.contains_key(&id) {
                    continue;
                }
                match Desktop::open(target, &view, proxy.clone()) {
                    Ok(desktop) => {
                        view.0.native_published.set(true);
                        let ready = view.0.native_ready.take();
                        if let Some(ready) = ready
                            && let Err(message) = ready(desktop.window.clone())
                        {
                            error = Some(message.clone());
                            fail(&view, message);
                            continue;
                        }
                        desktop.refresh();
                        desktops.insert(id, desktop);
                    }
                    Err(message) => {
                        error = Some(message.clone());
                        fail(&view, message);
                    }
                }
            }
            let result = catch_unwind(AssertUnwindSafe(|| match event {
                Event::UserEvent(Native::Wake(id)) => executor.ready(id),
                Event::UserEvent(Native::Refresh(id)) => {
                    if let Some(desktop) = desktops.get(&id) {
                        desktop.refresh();
                    }
                }
                Event::UserEvent(Native::Menu(id)) => {
                    for desktop in desktops.values() {
                        let key = desktop.view.0.menu_ids.borrow().get(&id).cloned();
                        if let Some(key) = key {
                            let _ = desktop.view.dispatch(Message::Menu(key));
                            break;
                        }
                    }
                }
                Event::WindowEvent {
                    window_id, event, ..
                } => {
                    if let Some((&id, desktop)) = desktops
                        .iter()
                        .find(|(_, desktop)| desktop.window.id() == window_id)
                    {
                        let callback = desktop.view.0.native_event.borrow().clone();
                        if let Some(callback) = callback {
                            callback(&event);
                        }
                        match event {
                            WindowEvent::CloseRequested => desktop.view.request_close(),
                            WindowEvent::Destroyed => desktop.view.close(),
                            WindowEvent::Focused(true) => {
                                for other in desktops.values() {
                                    other.active.set(false);
                                }
                                desktop.activate();
                                active = Some(id);
                                #[cfg(target_os = "windows")]
                                menu_owner.replace(Rc::downgrade(&desktop.view.0));
                                desktop.event(&event);
                            }
                            _ => desktop.event(&event),
                        }
                    }
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd",
                    target_os = "dragonfly"
                )))]
                Event::RedrawRequested(id) => {
                    if let Some(desktop) = desktops
                        .values_mut()
                        .find(|desktop| desktop.window.id() == id)
                        && let Err(e) = desktop.draw()
                    {
                        fail(&desktop.view, e);
                    }
                }
                Event::MainEventsCleared if executor.turn() => proxy.send(Native::Turn),
                _ => {}
            }));
            if let Err(panic) = result {
                error = Some(panic_error(panic));
                for desktop in desktops.values() {
                    desktop.view.close();
                }
            }
            desktops.retain(|_, desktop| {
                if desktop.view.0.lifecycle.is_closed() {
                    if error.is_none() {
                        error = desktop.view.0.fatal_error.borrow().clone();
                    }
                    false
                } else {
                    true
                }
            });
            host.views
                .borrow_mut()
                .retain(|(_, view)| !view.0.lifecycle.is_closed());
            if active.is_none_or(|id| !desktops.contains_key(&id)) {
                active = desktops.keys().next().copied();
                if let Some(desktop) = active.and_then(|id| desktops.get(&id)) {
                    desktop.activate();
                    #[cfg(target_os = "windows")]
                    menu_owner.replace(Rc::downgrade(&desktop.view.0));
                }
            }
            if host.views.borrow().is_empty() || error.is_some() {
                executor.close();
                *flow = ControlFlow::Exit;
            } else {
                let mut deadline = None;
                for desktop in desktops.values() {
                    let mut painter = desktop.painter.borrow_mut();
                    let due = painter.deadline.is_some_and(|at| at <= Instant::now());
                    if due {
                        painter.deadline = None;
                    }
                    if let Some(at) = painter.deadline {
                        deadline = Some(deadline.map_or(at, |old: Instant| old.min(at)));
                    }
                    drop(painter);
                    if due {
                        desktop.refresh();
                    }
                }
                *flow = deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil);
            }
        });
        if let Some(error) = error {
            return Err(error);
        }
        if status != 0 {
            return Err(format!(
                "Tao event loop exited with {status} (display disconnected)"
            ));
        }
        Ok(())
    }))
    .map_err(panic_error)?
}

#[cfg(test)]
mod rebuild_tests {
    use super::needs_rebuild;

    /// Steady input events must reuse the retained tree: only the first
    /// pump or a binding/structure move (revision) rebuilds. Rebuilding
    /// every tick is what stalled the whole window while Input was open.
    #[test]
    fn steady_events_reuse_the_retained_tree() {
        assert!(needs_rebuild(true, 0, 0));
        assert!(needs_rebuild(false, 1, 0));
        assert!(!needs_rebuild(false, 7, 7));
        assert!(!needs_rebuild(false, 7, 7));
    }
}
