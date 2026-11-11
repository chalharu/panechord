//! SDK-owned desktop windows. No Reactor publication or HWND discovery.
use crate::{
    Host, View, close::Gate, executor::Bridge, owned_bindings::*, view::owned_render::Renderer,
};
use panechord_runtime::{ViewCore, panic_message};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::{Rc, Weak},
};
use windows_core::{Array, Error, HSTRING, Interface, Ref, Result};
use windows_sys::Win32::{
    Foundation::{HWND as RawHwnd, RECT as RawRect},
    System::Threading::GetCurrentThreadId,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

thread_local! {
    static INITIAL_SIZE: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
    static CURRENT: RefCell<Weak<RunState>> = const { RefCell::new(Weak::new()) };
    static RUNNING: Cell<bool> = const { Cell::new(false) };
    static CALLBACK_DEPTH: Cell<usize> = const { Cell::new(0) };
}
struct NativeDepth(usize);
impl NativeDepth {
    fn enter() -> Self {
        Self(CALLBACK_DEPTH.with(|d| {
            let old = d.get();
            d.set(old.saturating_add(1));
            old
        }))
    }
}
impl Drop for NativeDepth {
    fn drop(&mut self) {
        CALLBACK_DEPTH.with(|d| d.set(self.0));
    }
}
pub(crate) fn sdk_error(message: impl AsRef<str>) -> Error {
    Error::new(
        windows_core::HRESULT(0x80004005u32 as i32),
        message.as_ref(),
    )
}
fn current() -> Result<Rc<RunState>> {
    CURRENT
        .with(|s| s.borrow().upgrade())
        .ok_or_else(|| sdk_error("no active owned driver"))
}
pub(crate) fn callback(work: impl FnOnce() -> Result<()>) {
    let _depth = NativeDepth::enter();
    let result = catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|p| {
        Err(sdk_error(format!(
            "native callback panic: {}",
            panic_message(&*p)
        )))
    });
    if let Err(error) = result
        && let Ok(state) = current()
    {
        state.defer_fault(error.to_string());
    }
}
pub(crate) fn with_renderer(
    generation: u64,
    work: impl FnOnce(&Renderer) -> Result<()>,
) -> Result<()> {
    let state = current()?;
    let native = state.natives.borrow().get(&generation).cloned();
    if let Some(native) = native
        && native.gate.alive()
        && !native.closing.get()
    {
        work(&native.renderer)?;
    }
    Ok(())
}
fn native_for(id: &str) -> Result<Rc<Native>> {
    let state = current()?;
    let native = state
        .natives
        .borrow()
        .values()
        .find(|n| n.id == id && !n.view.is_closed() && !n.closing.get() && n.gate.alive())
        .cloned();
    native.ok_or_else(|| sdk_error("native not ready"))
}
pub(crate) fn native_title(id: &str) -> std::result::Result<(u64, usize, String), String> {
    (|| -> Result<_> {
        let n = native_for(id)?;
        n.identity()?;
        let mut text = vec![0u16; 4096];
        let length = unsafe { GetWindowTextW(n.hwnd, text.as_mut_ptr(), text.len() as i32) };
        text.truncate(length as usize);
        let actual = String::from_utf16_lossy(&text);
        if actual != n.window.Title()? {
            return Err(sdk_error(
                "owned Window.Title differs from native HWND caption",
            ));
        }
        Ok((n.generation, n.hwnd as usize, actual))
    })()
    .map_err(|e| e.to_string())
}
pub(crate) fn request_native_close(id: &str) -> std::result::Result<(), String> {
    native_for(id)
        .map_err(|e| e.to_string())?
        .gate
        .request_native_close()
}
pub(crate) fn inject_rejected_wake() -> std::result::Result<(), String> {
    let s = current().map_err(|e| e.to_string())?;
    let bridge = s.bridge.borrow().clone().ok_or("no active bridge")?;
    bridge.inject_rejected_wake();
    Ok(())
}
pub(crate) fn native_assertions(id: &str) -> std::result::Result<(), String> {
    (|| -> Result<()> {
        let n = native_for(id)?;
        n.identity()?;
        if n.window.Content()? != n.renderer.root()? {
            return Err(sdk_error("Window.Content readback"));
        }
        if let Some(expected) = n.expected_client.get() {
            if n.first_client.get() != Some(expected) {
                return Err(sdk_error(format!(
                    "firstshow client {:?}, expected {expected:?}",
                    n.first_client.get()
                )));
            }
            println!(
                "NATIVE_FIRSTSHOW_PASS generation={} client={}x{} requested-size-once=true",
                n.generation, expected.0, expected.1
            );
        }
        eprintln!(
            "NATIVE_ASSERT_CONTEXT {}",
            native_focus_context(n.generation)?
        );
        n.renderer.assertions()
    })()
    .map_err(|e| e.to_string())
}
pub(crate) fn native_focus_context(generation: u64) -> Result<String> {
    let state = current()?;
    let native = state.natives.borrow().get(&generation).cloned();
    let native = native.ok_or_else(|| sdk_error("focus diagnostic native retired"))?;
    let thread = unsafe { GetCurrentThreadId() };
    let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    let available = unsafe { GetGUIThreadInfo(thread, &mut info) } != 0;
    Ok(format!(
        "id={} generation={generation} hwnd={:p} thread={thread} owner-thread={} gui-info={available} active={:p} focus={:p} foreground={:p}",
        native.id,
        native.hwnd,
        unsafe { GetWindowThreadProcessId(native.hwnd, std::ptr::null_mut()) },
        info.hwndActive,
        info.hwndFocus,
        unsafe { GetForegroundWindow() },
    ))
}
pub(crate) fn native_set_text(
    id: &str,
    node: &str,
    value: &str,
) -> std::result::Result<(), String> {
    native_for(id)
        .and_then(|n| n.renderer.set_text(node, value))
        .map_err(|e| e.to_string())
}
pub(crate) fn native_identity(id: &str, node: &str) -> std::result::Result<(u64, usize), String> {
    native_for(id)
        .and_then(|n| n.renderer.node_identity(node))
        .map_err(|e| e.to_string())
}
pub(crate) fn native_selectors(id: &str) -> std::result::Result<(), String> {
    native_for(id)
        .and_then(|n| n.renderer.selectors())
        .map_err(|e| e.to_string())
}
pub(crate) fn native_dialog(id: &str, open: bool) -> std::result::Result<(), String> {
    native_for(id)
        .and_then(|n| n.renderer.dialog_fact(open))
        .map_err(|e| e.to_string())
}
pub(crate) fn native_removed_token(id: &str, token: u64) -> std::result::Result<(), String> {
    native_for(id)
        .and_then(|n| n.renderer.removed_token(token))
        .map_err(|e| e.to_string())
}
pub(crate) fn native_hold_entry(id: &str, node: &str) -> std::result::Result<u64, String> {
    native_for(id)
        .and_then(|n| n.renderer.hold_entry(node))
        .map_err(|e| e.to_string())
}
pub(crate) fn native_request_focus(id: &str, node: &str) -> std::result::Result<(), String> {
    native_for(id)
        .and_then(|n| n.renderer.request_entry_focus(node))
        .map_err(|e| e.to_string())
}
pub(crate) fn native_focus_delivered(id: &str, node: &str) -> std::result::Result<bool, String> {
    native_for(id)
        .and_then(|n| n.renderer.entry_focus_delivered(node))
        .map_err(|e| e.to_string())
}
pub(crate) fn native_menu_edit_assertions(id: &str, node: &str) -> std::result::Result<(), String> {
    native_for(id)
        .and_then(|n| n.renderer.menu_edit_assertions(node))
        .map_err(|e| e.to_string())
}
pub(crate) fn diagnostic_text_probe(
    view: &View,
    entry: &str,
    label: &str,
    set_text: Option<&str>,
) -> std::result::Result<Option<crate::smoke::NativeTextProbe>, String> {
    let state = current().map_err(|e| e.to_string())?;
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<_> {
        if view.is_closed() {
            return Err(sdk_error("diagnostic View is closed"));
        }
        let native = state
            .natives
            .borrow()
            .values()
            .find(|n| {
                n.view.same_view(view) && !n.view.is_closed() && !n.closing.get() && n.gate.alive()
            })
            .cloned();
        let Some(n) = native else {
            return Ok(None);
        };
        n.identity()?;
        let mut readback = None;
        with_renderer(n.generation, |renderer| {
            if n.window.Content()? != renderer.root()? {
                return Err(sdk_error("diagnostic Window.Content identity"));
            }
            readback = renderer.diagnostic_text_probe(entry, label, set_text, n.hwnd as usize)?;
            Ok(())
        })?;
        n.identity()?;
        if n.closing.get() || view.is_closed() {
            return Err(sdk_error("diagnostic native retired during readback"));
        }
        Ok(readback)
    }))
    .unwrap_or_else(|p| {
        Err(sdk_error(format!(
            "diagnostic native panic: {}",
            panic_message(&*p)
        )))
    });
    if let Err(error) = &result {
        state.defer_fault(error.to_string());
    }
    result.map_err(|e| e.to_string())
}
pub(crate) fn set_initial_window_size(width: u32, height: u32) {
    if width > 0 && height > 0 && width <= 8192 && height <= 8192 {
        INITIAL_SIZE.with(|s| s.set(Some((width, height))));
    }
}

type Make = Box<dyn FnOnce(Rc<Host>) -> std::result::Result<(), String>>;
#[derive(Default)]
struct RunState {
    app: RefCell<Option<Application>>,
    resources: RefCell<Option<ResourceDictionary>>,
    make: RefCell<Option<Make>>,
    host: RefCell<Option<Host>>,
    bridge: RefCell<Option<Rc<Bridge>>>,
    natives: RefCell<BTreeMap<u64, Rc<Native>>>,
    next: Cell<u64>,
    pending: Cell<bool>,
    busy: Cell<bool>,
    stopping: Cell<bool>,
    launched: Cell<bool>,
    timer: Cell<usize>,
    error: RefCell<Option<String>>,
    last_dead: RefCell<Option<(u64, RawHwnd, Rc<Gate>)>>,
}
impl RunState {
    fn defer_fault(&self, error: String) {
        if self.error.borrow().is_none() {
            self.error.replace(Some(error));
        }
        self.pending.set(true);
    }
    fn host(&self) -> Option<Host> {
        self.host.borrow().clone()
    }
    fn next_generation(&self) -> Result<u64> {
        let next = self
            .next
            .get()
            .checked_add(1)
            .ok_or_else(|| sdk_error("window generation exhausted"))?;
        self.next.set(next);
        Ok(next)
    }
    fn launch(self: &Rc<Self>) -> Result<()> {
        if self.launched.replace(true) {
            return Err(sdk_error("duplicate OnLaunched"));
        }
        let bridge = Bridge::new().map_err(sdk_error)?;
        let host = Host::new(bridge.clone());
        self.bridge.replace(Some(bridge.clone()));
        self.host.replace(Some(host.clone()));
        let weak = Rc::downgrade(self);
        bridge.on_fault(Rc::new(move |e| {
            if let Some(s) = weak.upgrade() {
                s.defer_fault(e);
            }
        }));
        let weak = Rc::downgrade(self);
        host.set_changed(Rc::new(move || {
            if let Some(s) = weak.upgrade() {
                s.pending.set(true);
            }
        }));
        let timer = unsafe { SetTimer(std::ptr::null_mut(), 0, 25, Some(health_timer)) };
        if timer == 0 {
            return Err(sdk_error("SetTimer owned UI health"));
        }
        self.timer.set(timer);
        let make = self
            .make
            .take()
            .ok_or_else(|| sdk_error("make consumed twice"))?;
        if let Err(e) = host.make(move |h| make(Rc::new(h))) {
            self.defer_fault(e);
        }
        self.pending.set(true);
        self.turn()?;
        if self.natives.borrow().is_empty() {
            // Empty make has no desktop Window to trigger automatic shutdown.
            self.finish()?;
            self.app
                .borrow()
                .as_ref()
                .ok_or_else(|| sdk_error("missing Application"))?
                .Exit()?;
        }
        Ok(())
    }
    fn open(self: &Rc<Self>, id: String, view: View) -> Result<()> {
        if self.natives.borrow().len() >= 256 {
            return Err(sdk_error("native window budget"));
        }
        let generation = self.next_generation()?;
        let window = Window::new()?;
        let built = (|| -> Result<Rc<Native>> {
            let mut hwnd = std::ptr::null_mut();
            unsafe {
                window
                    .cast::<IWindowNative>()?
                    .WindowHandle(&mut hwnd)
                    .ok()?;
            }
            if unsafe { IsWindow(hwnd) } == 0
                || unsafe { IsWindowVisible(hwnd) } != 0
                || unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) }
                    != unsafe { GetCurrentThreadId() }
            {
                return Err(sdk_error(
                    "owned HWND must be live, hidden and UI-local before Activate",
                ));
            }
            let weak = view.downgrade();
            let gate = Gate::from_hwnd(hwnd, move || {
                callback(|| {
                    if let Some(v) = weak.upgrade() {
                        eprintln!("NATIVE_WM_CLOSE_VETO generation={generation} hwnd={hwnd:p}");
                        v.native_close_request();
                    }
                    Ok(())
                });
            })
            .map_err(sdk_error)?;
            if let Some((old, old_hwnd, dead)) = self.last_dead.borrow().as_ref() {
                if dead.matches(hwnd) || dead.alive() {
                    return Err(sdk_error("retired Gate authorized a new HWND"));
                }
                eprintln!(
                    "NATIVE_GATE_GENERATION old={old} new={generation} old-hwnd=0x{:x} new-hwnd={hwnd:p} recycled={} old-gate-no-match=true",
                    *old_hwnd as usize,
                    *old_hwnd == hwnd
                );
            }
            let renderer = Renderer::new(
                view.clone(),
                generation,
                self.bridge
                    .borrow()
                    .clone()
                    .ok_or_else(|| sdk_error("missing Bridge"))?,
            )?;
            let native = Rc::new(Native {
                generation,
                id,
                view,
                window: window.clone(),
                hwnd,
                gate,
                renderer,
                closing: Cell::new(false),
                refresh: Cell::new(true),
                present: Cell::new(false),
                first: Cell::new(false),
                initial_size: Cell::new(INITIAL_SIZE.with(Cell::take)),
                initial_outer: Cell::new(None),
                first_client: Cell::new(None),
                expected_client: Cell::new(None),
            });
            self.natives.borrow_mut().insert(generation, native.clone());
            if unsafe {
                SetWindowSubclass(
                    hwnd,
                    Some(first_show),
                    generation as usize,
                    generation as usize,
                )
            } == 0
            {
                return Err(sdk_error("SetWindowSubclass owned first-show"));
            }
            let weak = Rc::downgrade(&native);
            native.view.set_native_close(Rc::new(move || {
                if let Some(n) = weak.upgrade() {
                    n.closing.set(true);
                }
            }));
            let weak = Rc::downgrade(&native);
            native.view.set_invalidate(Rc::new(move || {
                if let Some(n) = weak.upgrade() {
                    n.refresh.set(true);
                }
            }));
            let weak = Rc::downgrade(&native);
            native.view.set_present(Rc::new(move || {
                if let Some(n) = weak.upgrade() {
                    n.present.set(true);
                }
            }));
            window.SetTitle(&native.view.title())?;
            eprintln!("PROJECTION generation={generation} render-start owned-native");
            native.renderer.apply()?;
            window.SetContent(&native.renderer.root()?)?;
            eprintln!("PROJECTION generation={generation} render-complete owned-native");
            eprintln!(
                "NATIVE_OWNED generation={generation} hwnd={hwnd:p} previsible=0 gate-before-Activate"
            );
            window.Activate()?;
            native.identity()?;
            Ok(native)
        })();
        if let Err(error) = built {
            self.defer_fault(error.to_string());
            if let Err(close) = window.Close() {
                self.defer_fault(close.to_string());
            }
            return Err(error);
        }
        Ok(())
    }
    fn reap(&self) -> Result<()> {
        let dead: Vec<_> = self
            .natives
            .borrow()
            .values()
            .filter(|n| !n.gate.alive())
            .cloned()
            .collect();
        for n in dead {
            if n.gate.matches(n.hwnd) {
                return Err(sdk_error("Gate retirement before NCDESTROY"));
            }
            self.natives.borrow_mut().remove(&n.generation);
            self.last_dead
                .replace(Some((n.generation, n.hwnd, n.gate.clone())));
            eprintln!(
                "NATIVE_NCDESTROY generation={} hwnd={:p}",
                n.generation, n.hwnd
            );
            n.renderer.retire()?;
            n.view.clear_invalidate();
            n.view.native_closed();
            self.pending.set(true);
        }
        Ok(())
    }
    fn reconcile(self: &Rc<Self>) -> Result<()> {
        for _ in 0..64 {
            if !self.pending.replace(false) {
                return Ok(());
            }
            if let Some(host) = self.host() {
                for (id, view) in host.registered_views() {
                    let exists = self
                        .natives
                        .borrow()
                        .values()
                        .any(|n| n.id == id && n.view.same_view(&view));
                    if !exists {
                        self.open(id, view)?;
                    }
                }
            }
        }
        if self.pending.get() {
            return Err(sdk_error(
                "owned registration reconciliation did not settle",
            ));
        }
        Ok(())
    }
    fn turn(self: &Rc<Self>) -> Result<()> {
        if CALLBACK_DEPTH.with(|d| d.get() > 1) {
            return Ok(());
        }
        if self.busy.replace(true) {
            return Ok(());
        }
        struct Busy<'a>(&'a Cell<bool>);
        impl Drop for Busy<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _busy = Busy(&self.busy);
        self.reap()?;
        let error = self.error.borrow().clone().or_else(|| {
            self.host().and_then(|h| {
                h.error()
                    .or_else(|| h.building_views().iter().find_map(ViewCore::error))
            })
        });
        if let Some(error) = error {
            self.defer_fault(error);
            self.stopping.set(true);
            if let Some(h) = self.host() {
                h.fail(self.error.borrow().clone().unwrap());
            }
        }
        if !self.stopping.get() {
            // New registrations precede old closes, including onClose reentrancy.
            self.reconcile()?;
            let natives: Vec<_> = self.natives.borrow().values().cloned().collect();
            for n in natives {
                if !n.closing.get() && !n.view.is_closed() {
                    if n.refresh.replace(false) {
                        n.window.SetTitle(&n.view.title())?;
                        n.renderer.apply()?;
                    }
                    n.renderer.dialogs()?;
                    if n.present.replace(false) {
                        n.window.Activate()?;
                    }
                }
            }
            self.reconcile()?;
        }
        let natives: Vec<_> = self.natives.borrow().values().cloned().collect();
        let closing: Vec<_> = natives
            .iter()
            .filter(|n| self.stopping.get() || n.closing.get() || n.view.is_closed())
            .cloned()
            .collect();
        if !natives.is_empty() && closing.len() == natives.len() {
            self.finish()?;
        }
        for n in closing {
            n.closing.set(true);
            n.renderer.hide_dialogs()?;
            n.renderer.quiesce();
            n.gate.approve();
            eprintln!(
                "NATIVE_APPROVED_CLOSE generation={} queued-turn=true",
                n.generation
            );
            n.window.Close()?;
        }
        self.reap()?;
        if self.stopping.get()
            && let Some(app) = self.app.borrow().clone()
        {
            app.Exit()?;
        } // fatal only
        Ok(())
    }
    fn finish(&self) -> Result<()> {
        if let Some(host) = self.host() {
            host.close();
        }
        if let Some(bridge) = self.bridge.take() {
            bridge.shutdown();
        }
        let timer = self.timer.replace(0);
        if timer != 0 && unsafe { KillTimer(std::ptr::null_mut(), timer) } == 0 {
            return Err(sdk_error("KillTimer owned UI health"));
        }
        Ok(())
    }
    fn rollback(&self) {
        // Start may return with an error before the health timer can retire
        // windows. Never drop an owned Gate while its HWND is still alive.
        let natives: Vec<_> = self.natives.borrow().values().cloned().collect();
        for n in natives {
            n.closing.set(true);
            if let Err(error) = n.renderer.hide_dialogs() {
                self.defer_fault(error.to_string());
            }
            n.renderer.quiesce();
            if let Err(error) = n.gate.force_close() {
                self.defer_fault(error);
            }
            if n.gate.alive()
                && let Err(error) = n.window.Close()
            {
                self.defer_fault(error.to_string());
            }
        }
        if let Err(error) = self.reap() {
            self.defer_fault(error.to_string());
        }
    }
    fn validate_return(&self) -> Result<()> {
        if !self.launched.get() || self.make.borrow().is_some() {
            return Err(sdk_error(
                "Application returned before make/OnLaunched completed",
            ));
        }
        if !self.natives.borrow().is_empty()
            || self.bridge.borrow().is_some()
            || self.timer.get() != 0
        {
            return Err(sdk_error(
                "Application returned with live native instances, Bridge or health timer",
            ));
        }
        if !self
            .host()
            .is_some_and(|host| host.is_closed() && host.registered_views().is_empty())
        {
            return Err(sdk_error(
                "Application returned before Host registrations retired",
            ));
        }
        Ok(())
    }
}
struct Native {
    generation: u64,
    id: String,
    view: View,
    window: Window,
    hwnd: RawHwnd,
    gate: Rc<Gate>,
    renderer: Renderer,
    closing: Cell<bool>,
    refresh: Cell<bool>,
    present: Cell<bool>,
    first: Cell<bool>,
    initial_size: Cell<Option<(u32, u32)>>,
    initial_outer: Cell<Option<(i32, i32)>>,
    first_client: Cell<Option<(i32, i32)>>,
    expected_client: Cell<Option<(i32, i32)>>,
}
impl Native {
    fn identity(&self) -> Result<()> {
        let mut hwnd = std::ptr::null_mut();
        unsafe {
            self.window
                .cast::<IWindowNative>()?
                .WindowHandle(&mut hwnd)
                .ok()?;
        }
        if hwnd != self.hwnd || !self.gate.matches(hwnd) || !self.gate.alive() {
            return Err(sdk_error("owned HWND identity retired"));
        }
        Ok(())
    }
    fn prepare_size(&self) -> Result<()> {
        if let Some((width, height)) = self.initial_size.take() {
            let dpi = unsafe { GetDpiForWindow(self.hwnd) };
            if dpi == 0 {
                return Err(sdk_error("GetDpiForWindow"));
            }
            let px = |v| {
                i32::try_from((u64::from(v) * u64::from(dpi) + 48) / 96)
                    .map_err(|_| sdk_error("initial DPI overflow"))
            };
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: px(width)?,
                bottom: px(height)?,
            };
            self.expected_client.set(Some((rect.right, rect.bottom)));
            let ok = unsafe {
                AdjustWindowRectExForDpi(
                    &mut rect,
                    GetWindowLongW(self.hwnd, GWL_STYLE) as u32,
                    (!GetMenu(self.hwnd).is_null()).into(),
                    GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32,
                    dpi,
                )
            };
            if !ok.as_bool() {
                return Err(sdk_error("AdjustWindowRectExForDpi"));
            }
            self.initial_outer
                .set(Some((rect.right - rect.left, rect.bottom - rect.top)));
        }
        Ok(())
    }
}
unsafe extern "system" fn first_show(
    hwnd: RawHwnd,
    message: u32,
    wp: usize,
    lp: isize,
    id: usize,
    _: usize,
) -> isize {
    let _depth = NativeDepth::enter();
    callback(|| {
        let s = current()?;
        let native = s.natives.borrow().get(&(id as u64)).cloned();
        if let Some(n) = native
            && n.gate.matches(hwnd)
        {
            if message == WM_WINDOWPOSCHANGING
                && let Some(pos) = unsafe { (lp as *mut WINDOWPOS).as_mut() }
                && pos.flags & SWP_SHOWWINDOW != 0
                && !n.first.get()
            {
                n.prepare_size()?;
                if let Some((w, h)) = n.initial_outer.get() {
                    pos.cx = w;
                    pos.cy = h;
                    pos.flags &= !SWP_NOSIZE;
                }
            }
            if message == WM_WINDOWPOSCHANGED
                && unsafe { IsWindowVisible(hwnd) } != 0
                && !n.first.replace(true)
            {
                let mut r: RawRect = unsafe { std::mem::zeroed() };
                if unsafe { GetClientRect(hwnd, &mut r) } == 0 {
                    return Err(sdk_error("firstshow GetClientRect"));
                }
                n.first_client
                    .set(Some((r.right - r.left, r.bottom - r.top)));
                println!(
                    "READY id={} generation={} hwnd=0x{:x} previsible=0 owned=true first-client={}x{}",
                    n.id,
                    n.generation,
                    hwnd as usize,
                    r.right - r.left,
                    r.bottom - r.top
                );
            }
            if message == WM_NCDESTROY {
                unsafe {
                    RemoveWindowSubclass(hwnd, Some(first_show), id);
                }
            }
        }
        Ok(())
    });
    let result = unsafe { DefSubclassProc(hwnd, message, wp, lp) };
    // Amend the same pending first-show position after WinUI adjusts it;
    // no nested SetWindowPos and no arbitrary HWND selection.
    if message == WM_WINDOWPOSCHANGING {
        callback(|| {
            let s = current()?;
            let native = s.natives.borrow().get(&(id as u64)).cloned();
            if let Some(n) = native
                && n.gate.matches(hwnd)
                && !n.first.get()
                && let Some(pos) = unsafe { (lp as *mut WINDOWPOS).as_mut() }
                && pos.flags & SWP_SHOWWINDOW != 0
                && let Some((w, h)) = n.initial_outer.take()
            {
                pos.cx = w;
                pos.cy = h;
                pos.flags &= !SWP_NOSIZE;
            }
            Ok(())
        });
    }
    result
}
unsafe extern "system" fn health_timer(_: RawHwnd, _: u32, _: usize, _: u32) {
    callback(|| current()?.turn());
}
#[windows_core::implement(IApplicationOverrides, IXamlMetadataProvider)]
struct OwnedApplication {
    provider: IXamlMetadataProvider,
}
#[allow(non_snake_case)]
impl IApplicationOverrides_Impl for OwnedApplication_Impl {
    fn OnLaunched(&self, _: Ref<LaunchActivatedEventArgs>) -> Result<()> {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let s = current()?;
            let app = s
                .app
                .borrow()
                .clone()
                .ok_or_else(|| sdk_error("Application missing"))?;
            let resources = XamlControlsResources::new()?.cast::<ResourceDictionary>()?;
            app.Resources()?.MergedDictionaries()?.Append(&resources)?;
            s.resources.replace(Some(resources));
            s.launch()
        }))
        .unwrap_or_else(|_| Err(sdk_error("OnLaunched panicked")));
        if let Err(e) = &result {
            callback(|| {
                let s = current()?;
                s.defer_fault(e.to_string());
                s.turn()
            });
        }
        // Preserve the first error for driver teardown, rather than throwing an
        // unhandled native XAML override failure during owned-window rollback.
        Ok(())
    }
}
fn metadata<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    match catch_unwind(AssertUnwindSafe(work)) {
        Ok(r) => r,
        Err(_) => {
            callback(|| Err(sdk_error("metadata callback panicked")));
            Err(sdk_error("metadata callback panicked"))
        }
    }
}
#[allow(non_snake_case)]
impl IXamlMetadataProvider_Impl for OwnedApplication_Impl {
    fn GetXamlType(&self, name: &TypeName) -> Result<IXamlType> {
        metadata(|| self.provider.GetXamlType(name))
    }
    fn GetXamlTypeByFullName(&self, name: &HSTRING) -> Result<IXamlType> {
        metadata(|| self.provider.GetXamlTypeByFullName(&name.to_string_lossy()))
    }
    fn GetXmlnsDefinitions(&self) -> Result<Array<XmlnsDefinition>> {
        metadata(|| self.provider.GetXmlnsDefinitions())
    }
}
pub fn run_host<M>(make: M) -> std::result::Result<(), String>
where
    M: FnOnce(Rc<Host>) -> std::result::Result<(), String> + 'static,
{
    if RUNNING.with(|s| s.replace(true)) {
        return Err("owned backend already running".into());
    }
    struct Running;
    impl Drop for Running {
        fn drop(&mut self) {
            CURRENT.with(|s| s.replace(Weak::new()));
            RUNNING.with(|s| s.set(false));
        }
    }
    let _running = Running;
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32)
            .ok()
            .map_err(|e| e.to_string())?;
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    let _apartment = Apartment;
    let state = Rc::new(RunState::default());
    state.make.replace(Some(Box::new(make)));
    CURRENT.with(|s| s.replace(Rc::downgrade(&state)));
    let result = catch_unwind(AssertUnwindSafe(|| {
        Application::Start(&ApplicationInitializationCallback::new(|_| {
            callback(|| {
                let s = current()?;
                let provider =
                    XamlControlsXamlMetaDataProvider::new()?.cast::<IXamlMetadataProvider>()?;
                let app = Application::compose(OwnedApplication { provider })?;
                s.app.replace(Some(app));
                Ok(())
            })
        }))
    }));
    match result {
        Ok(Ok(())) => (),
        Ok(Err(e)) => state.defer_fault(e.to_string()),
        Err(p) => state.defer_fault(format!("Application.Start panic: {}", panic_message(&*p))),
    }
    if let Err(e) = state.reap() {
        state.defer_fault(e.to_string());
    }
    if !state.natives.borrow().is_empty() {
        state.defer_fault("Application returned before all owned HWNDs reached NCDESTROY".into());
        state.rollback();
    }
    if let Err(e) = state.finish().and_then(|()| state.validate_return()) {
        state.defer_fault(e.to_string());
    }
    let remaining = state.natives.borrow().len();
    if remaining != 0 {
        state.defer_fault(format!(
            "owned native teardown retained {remaining} instances"
        ));
    }
    let error = state
        .error
        .borrow()
        .clone()
        .or_else(|| state.host().and_then(|h| h.error()));
    if error.is_some() && remaining == 0 && state.bridge.borrow().is_none() {
        println!("FAULT_TEARDOWN_PASS native-instances=0 driver-bridge-retired=true");
    }
    if error.is_none() {
        if state.next.get() == 0 {
            println!("NATIVE_EMPTY_STARTUP_PASS no-desktop-window make-once host-retired=true");
        } else {
            println!(
                "NATIVE_LIFECYCLE_PASS native-instances=0 registrations=0 bridge-retired=true natural-Start-return"
            );
        }
    }
    state.resources.take();
    state.app.take();
    error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generation_exhaustion_is_checked_and_first_fault_wins() {
        let s = RunState::default();
        assert_eq!(s.next_generation().unwrap(), 1);
        s.next.set(u64::MAX);
        assert!(s.next_generation().is_err());
        s.defer_fault("first".into());
        s.defer_fault("later".into());
        assert_eq!(s.error.borrow().as_deref(), Some("first"));
    }
    #[test]
    fn initial_size_is_consumed_once() {
        let saved = INITIAL_SIZE.with(Cell::take);
        set_initial_window_size(640, 480);
        for (width, height) in [
            (0, 480),
            (640, 0),
            (u32::MAX, 480),
            (640, u32::MAX),
            (8193, 480),
            (640, 8193),
        ] {
            set_initial_window_size(width, height);
            assert_eq!(INITIAL_SIZE.with(Cell::get), Some((640, 480)));
        }
        assert_eq!(INITIAL_SIZE.with(Cell::take), Some((640, 480)));
        assert_eq!(INITIAL_SIZE.with(Cell::take), None);
        INITIAL_SIZE.with(|s| s.set(saved));
    }
    #[test]
    fn nested_native_callback_defers_retirement_and_depth_restores() {
        let before = CALLBACK_DEPTH.with(Cell::get);
        let outer = NativeDepth::enter();
        let inner = NativeDepth::enter();
        let s = Rc::new(RunState::default());
        s.stopping.set(true);
        assert!(s.turn().is_ok());
        assert!(!s.busy.get());
        drop(inner);
        drop(outer);
        assert_eq!(CALLBACK_DEPTH.with(Cell::get), before);
    }
    #[test]
    fn lifecycle_marker_requires_completed_make_and_retired_host() {
        let state = RunState::default();
        assert!(state.validate_return().is_err());
        state.launched.set(true);
        assert!(state.validate_return().is_err());
        state.make.replace(Some(Box::new(|_| Ok(()))));
        assert!(state.validate_return().is_err());
        state.make.take();
        state.timer.set(1);
        assert!(state.validate_return().is_err());
        state.timer.set(0);
    }
}
