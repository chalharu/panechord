//! Experimental UI-local executor. The host's WinUI loop dispatches this message-only HWND.
//! No Rc or Future crosses threads; HWND posting and retirement share a lock.
use futures::{Stream, future::LocalBoxFuture, stream::FuturesUnordered, task::{ArcWake, waker}};
use panechord_runtime::Executor;
use std::{cell::{Cell, RefCell}, collections::HashMap, num::NonZeroUsize, pin::Pin,
    rc::{Rc, Weak}, sync::{Arc, Mutex}, task::{Context, Poll}, ptr::null_mut};
use windows_sys::Win32::{Foundation::*, System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*};

const READY: u32 = WM_APP + 71;
const HEALTH: usize = 72;
type FaultHandler = Rc<dyn Fn(String)>;
thread_local! {
    static BRIDGES: RefCell<HashMap<usize, Weak<Bridge>>> = RefCell::new(HashMap::new());
}
#[derive(Default)]
struct PostState { hwnd: Option<usize>, pending: bool }
#[derive(Default)]
struct Notify { state: Mutex<PostState>, failed: std::sync::atomic::AtomicBool,
    reject_next: std::sync::atomic::AtomicBool }
impl ArcWake for Notify {
    fn wake_by_ref(this: &Arc<Self>) {
        let mut state = this.state.lock().unwrap();
        if let Some(hwnd) = state.hwnd.filter(|_| !state.pending) {
            // Holding the retirement lock prevents a timer posting to a recycled HWND.
            if this.reject_next.swap(false, std::sync::atomic::Ordering::AcqRel)
                || unsafe { PostMessageW(hwnd as HWND, READY, 0, 0) } == 0 {
                this.failed.store(true, std::sync::atomic::Ordering::Release);
            } else { state.pending = true; }
        }
    }
}
pub struct Bridge {
    notify: Arc<Notify>,
    tasks: RefCell<FuturesUnordered<LocalBoxFuture<'static, ()>>>,
    incoming: RefCell<Vec<LocalBoxFuture<'static, ()>>>,
    polling: Cell<bool>,
    closed: Cell<bool>,
    fault: RefCell<Option<String>>,
    on_fault: RefCell<Option<FaultHandler>>,
    reported: Cell<bool>,
}
impl Bridge {
    pub fn new() -> Result<Rc<Self>, String> {
        let class: Vec<u16> = "PanechordWindowsP0Wake\0".encode_utf16().collect();
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: instance,
            lpszClassName: class.as_ptr(), ..unsafe { std::mem::zeroed() } };
        if unsafe { RegisterClassW(&wc) } == 0 && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
            return Err(format!("RegisterClassW: {}", unsafe { GetLastError() }));
        }
        let hwnd = unsafe { CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0,
            0, 0, 0, 0, HWND_MESSAGE, null_mut(), instance, std::ptr::null()) };
        if hwnd.is_null() { return Err(format!("CreateWindowExW: {}", unsafe { GetLastError() })); }
        let bridge = Rc::new(Self { notify: Arc::default(), tasks: RefCell::default(),
            incoming: RefCell::default(), polling: Cell::new(false), closed: Cell::new(false),
            fault: RefCell::default(), on_fault: RefCell::default(), reported: Cell::new(false) });
        bridge.notify.state.lock().unwrap().hwnd = Some(hwnd as usize);
        BRIDGES.with(|all| all.borrow_mut().insert(hwnd as usize, Rc::downgrade(&bridge)));
        // Timers are synthesized by the UI loop, independent of PostMessage's queue quota.
        if unsafe { SetTimer(hwnd, HEALTH, 50, None) } == 0 {
            let error = unsafe { GetLastError() };
            bridge.shutdown();
            return Err(format!("SetTimer health check: {error}"));
        }
        Ok(bridge)
    }
    pub fn on_fault(&self, callback: FaultHandler) {
        let old = self.on_fault.replace(Some(callback)); drop(old);
        self.check_fault();
    }
    fn check_fault(&self) {
        if let Some(error) = self.fault() {
            let callback = self.on_fault.borrow().clone();
            if let Some(callback) = callback && !self.reported.replace(true)
                && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(error))).is_err() {
                eprintln!("P0 host fault callback panicked");
            }
            self.shutdown();
        }
    }
    /// Deterministic fixture injection through the same notifier rejection branch.
    pub fn inject_rejected_wake(&self) {
        self.notify.reject_next.store(true, std::sync::atomic::Ordering::Release);
        ArcWake::wake_by_ref(&self.notify);
    }
    fn turn(&self) {
        self.notify.state.lock().unwrap().pending = false;
        self.check_fault();
        if self.closed.get() || self.polling.replace(true) { return; }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Poll outside RefCell borrows: spawn-during-poll and callbacks can reenter.
            let mut tasks = self.tasks.take();
            let new = self.incoming.take();
            for task in new { tasks.push(task); }
            let wake = waker(self.notify.clone());
            let mut context = Context::from_waker(&wake);
            for count in 0..64 {
                match Pin::new(&mut tasks).poll_next(&mut context) {
                    Poll::Ready(Some(())) if count == 63 => ArcWake::wake_by_ref(&self.notify),
                    Poll::Ready(Some(())) => (),
                    Poll::Pending | Poll::Ready(None) => break,
                }
            }
            if !self.closed.get() { drop(self.tasks.replace(tasks)); }
        }));
        self.polling.set(false);
        if result.is_err() {
            self.fault.replace(Some("local task panicked".into()));
        }
        self.check_fault();
    }
    pub fn fault(&self) -> Option<String> {
        if self.notify.failed.load(std::sync::atomic::Ordering::Acquire) {
            Some("PostMessageW rejected wake".into())
        } else { self.fault.borrow().clone() }
    }
    pub fn shutdown(&self) {
        if self.closed.replace(true) { return; }
        let hwnd = self.notify.state.lock().unwrap().hwnd.take();
        if let Some(hwnd) = hwnd {
            BRIDGES.with(|all| all.borrow_mut().remove(&hwnd));
            unsafe { KillTimer(hwnd as HWND, HEALTH); }
            unsafe { DestroyWindow(hwnd as HWND); }
        }
        drop(self.incoming.take());
        drop(self.tasks.take());
    }
}
impl Executor for Bridge {
    fn spawn(&self, future: LocalBoxFuture<'static, ()>) {
        if self.closed.get() { return; }
        self.incoming.borrow_mut().push(future);
        ArcWake::wake_by_ref(&self.notify);
        self.check_fault();
    }
    fn dispatch_budget(&self) -> Option<NonZeroUsize> { NonZeroUsize::new(64) }
}
impl Drop for Bridge { fn drop(&mut self) { self.shutdown(); } }
unsafe extern "system" fn wndproc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if message == READY || message == WM_TIMER && wp == HEALTH {
        let bridge = BRIDGES.with(|all| all.borrow().get(&(hwnd as usize)).and_then(Weak::upgrade));
        if let Some(bridge) = bridge {
            if message == READY { bridge.turn(); } else { bridge.check_fault(); }
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, message, wp, lp) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{FutureExt, future::poll_fn};
    use std::time::{Duration, Instant};
    // Test-only loop, never nested inside App::run. Wait for messages, not a fixed sleep.
    fn until(done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done() {
            assert!(Instant::now() < deadline, "local executor watchdog expired");
            unsafe {
                MsgWaitForMultipleObjectsEx(0, std::ptr::null(), 50, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    assert!(Instant::now() < deadline, "message flood watchdog expired");
                    TranslateMessage(&msg); DispatchMessageW(&msg);
                }
            }
        }
    }
    #[test]
    fn non_send_timer_and_spawn_during_poll() {
        let bridge = Bridge::new().unwrap();
        let count = Rc::new(Cell::new(0));
        let out = count.clone(); let nested = bridge.clone();
        bridge.spawn(async move {
            futures_timer::Delay::new(Duration::from_millis(15)).await;
            out.set(out.get() + 1);
            nested.spawn(async move { out.set(out.get() + 1); }.boxed_local());
        }.boxed_local());
        until(|| count.get() == 2);
        assert_eq!(bridge.fault(), None);
    }
    #[test]
    fn self_wake_and_shutdown_drop() {
        let bridge = Bridge::new().unwrap(); let done = Rc::new(Cell::new(false));
        let out = done.clone(); let mut polls = 0;
        bridge.spawn(async move {
            poll_fn(move |cx| { polls += 1; if polls == 150 { Poll::Ready(()) }
                else { cx.waker().wake_by_ref(); Poll::Pending } }).await;
            out.set(true);
        }.boxed_local());
        until(|| done.get());
        let store = panechord_runtime::Store::new(0u32, bridge.clone());
        let binding = store.bind(|s| *s, Some(|s, value| *s = value));
        let feedback = binding.clone();
        let _subscription = binding.subscribe(Rc::new(move |v| { if v < 200 { let _ = feedback.set(v + 1); } }));
        bridge.shutdown();
        assert!(store.is_closed()); assert!(store.dispatch_failed());
    }
    #[test]
    fn two_bridges_one_retired_late_timer_wake() {
        let first = Bridge::new().unwrap(); let second = Bridge::new().unwrap();
        let done = Rc::new(Cell::new(false)); let out = done.clone();
        let saved = Arc::new(Mutex::new(None)); let save = saved.clone();
        first.spawn(poll_fn(move |cx| {
            *save.lock().unwrap() = Some(cx.waker().clone()); Poll::Pending
        }).boxed_local());
        until(|| saved.lock().unwrap().is_some());
        first.shutdown();
        std::thread::spawn(move || {
            let wake = saved.lock().unwrap().take().unwrap();
            for _ in 0..10_000 { wake.wake_by_ref(); }
        }).join().unwrap();
        second.spawn(async move { futures_timer::Delay::new(Duration::from_millis(15)).await;
            out.set(true); }.boxed_local());
        until(|| done.get()); assert_eq!(second.fault(), None);
    }
    #[test]
    fn session_quit_veto_then_two_window_commit() {
        use panechord_runtime::{Session, ShutdownOutcome, ShutdownPhase, WindowHooks};
        let bridge = Bridge::new().unwrap(); let session = Session::new();
        let allow = Rc::new(Cell::new(false)); let closed = Rc::new(Cell::new(0));
        let mut tokens = Vec::new();
        for id in ["first", "second"] {
            let approval = allow.clone(); let close_count = closed.clone();
            tokens.push(session.register_window(id, WindowHooks {
                confirm: Rc::new(move || { let approval = approval.clone(); async move {
                    futures_timer::Delay::new(Duration::from_millis(5)).await; approval.get()
                }.boxed_local() }),
                cancel: Rc::new(|| ()), drain: Rc::new(|| async {}.boxed_local()),
                close: Rc::new(move || close_count.set(close_count.get() + 1)),
            }).unwrap());
        }
        let outcome = Rc::new(RefCell::new(None)); let out = outcome.clone(); let request = session.clone();
        bridge.spawn(async move { out.replace(Some(request.shutdown().request_shutdown(Duration::from_secs(1)).await)); }.boxed_local());
        until(|| outcome.borrow().is_some());
        assert_eq!(outcome.take(), Some(ShutdownOutcome::Cancelled));
        assert_eq!(session.shutdown().phase(), ShutdownPhase::Running); assert_eq!(closed.get(), 0);
        allow.set(true); let out = outcome.clone(); let request = session.clone();
        bridge.spawn(async move { out.replace(Some(request.shutdown().request_shutdown(Duration::from_secs(1)).await)); }.boxed_local());
        until(|| outcome.borrow().is_some());
        assert_eq!(outcome.take(), Some(ShutdownOutcome::Closed)); assert_eq!(closed.get(), 2);
        drop(tokens);
    }
    #[test]
    fn completed_tasks_yield_after_at_most_64() {
        let bridge = Bridge::new().unwrap(); let done = Rc::new(Cell::new(0));
        for _ in 0..1_000 { let out = done.clone(); bridge.spawn(async move { out.set(out.get() + 1); }.boxed_local()); }
        let hwnd = bridge.notify.state.lock().unwrap().hwnd.unwrap() as HWND;
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            assert_ne!(PeekMessageW(&mut msg, hwnd, READY, READY, PM_REMOVE), 0);
            DispatchMessageW(&msg);
        }
        assert!(done.get() > 0 && done.get() <= 64);
        until(|| done.get() == 1_000);
    }
    #[test]
    fn wake_coalescing_and_invalid_handle_rejection() {
        let bridge = Bridge::new().unwrap();
        for _ in 0..10_000 { ArcWake::wake_by_ref(&bridge.notify); }
        let hwnd = bridge.notify.state.lock().unwrap().hwnd.unwrap() as HWND;
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            assert_ne!(PeekMessageW(&mut msg, hwnd, READY, READY, PM_REMOVE), 0);
            let mut other: MSG = std::mem::zeroed();
            assert_eq!(PeekMessageW(&mut other, hwnd, READY, READY, PM_REMOVE), 0);
            DispatchMessageW(&msg);
        }
        let rejected = Arc::new(Notify::default());
        // Do not use -1: that is HWND_BROADCAST, not an invalid window handle.
        let invalid = 0x1234_5678usize;
        assert_eq!(unsafe { IsWindow(invalid as HWND) }, 0);
        rejected.state.lock().unwrap().hwnd = Some(invalid);
        ArcWake::wake_by_ref(&rejected);
        assert!(rejected.failed.load(std::sync::atomic::Ordering::Acquire));
        assert!(!rejected.state.lock().unwrap().pending);
    }
    #[test]
    fn task_panic_is_contained_and_stops_executor() {
        let bridge = Bridge::new().unwrap();
        bridge.spawn(async { panic!("intentional P0 task panic"); }.boxed_local());
        until(|| bridge.fault().is_some());
        assert!(bridge.closed.get()); assert_eq!(bridge.fault().as_deref(), Some("local task panicked"));
    }
    #[test]
    fn rejected_wake_notifies_host_without_another_spawn() {
        let bridge = Bridge::new().unwrap();
        let faults = Rc::new(RefCell::new(Vec::new())); let out = faults.clone();
        bridge.on_fault(Rc::new(move |error| out.borrow_mut().push(error)));
        bridge.inject_rejected_wake();
        until(|| !faults.borrow().is_empty());
        assert_eq!(faults.borrow().as_slice(), ["PostMessageW rejected wake"]);
        assert!(bridge.closed.get());
    }
    #[test]
    fn panic_notifies_host_once_before_shutdown() {
        let bridge = Bridge::new().unwrap(); let count = Rc::new(Cell::new(0)); let out = count.clone();
        bridge.on_fault(Rc::new(move |error| { assert_eq!(error, "local task panicked"); out.set(out.get() + 1); }));
        bridge.spawn(async { panic!("intentional P0 host-notified panic"); }.boxed_local());
        until(|| count.get() == 1);
        bridge.check_fault(); assert_eq!(count.get(), 1); assert!(bridge.closed.get());
    }
}
