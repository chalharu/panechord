//! UI-local host. Adapted from the P1 host; native publication belongs to the driver.
use super::View;
use futures::{
    FutureExt,
    future::{AbortHandle, Abortable, LocalBoxFuture, select},
};
use panechord_runtime::{
    Binding, Executor, Session, ShutdownPhase, Subscription, UiHost, ViewCore, WindowHooks,
    WindowHost, panic_message,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::{Rc, Weak},
    time::Duration,
};

#[derive(Clone)]
pub struct Host(pub(super) Rc<HostInner>);
pub(super) struct HostInner {
    executor: Rc<dyn Executor>,
    session: RefCell<Option<Session>>,
    slots: RefCell<BTreeMap<String, Rc<Slot>>>,
    building: RefCell<Vec<View>>,
    made: Cell<bool>,
    making: Cell<bool>,
    closed: Cell<bool>,
    error: RefCell<Option<String>>,
    close_timeout: Cell<Duration>,
    changed: RefCell<Option<Rc<dyn Fn()>>>,
    notifying: Cell<bool>,
    pending_notification: Cell<bool>,
}
struct Slot {
    view: View,
    hooks: WindowHooks,
    registration: RefCell<Option<Subscription>>,
    active: Cell<bool>,
    requesting: Cell<bool>,
    abort: RefCell<Option<AbortHandle>>,
}
impl Slot {
    fn retire(&self) {
        self.active.set(false);
        self.requesting.set(false);
        let abort = self.abort.take();
        let token = self.registration.take();
        if let Some(abort) = abort {
            abort.abort();
        }
        drop(token);
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        self.retire();
    }
}
thread_local! {
    static MAKING: RefCell<Vec<Weak<HostInner>>> = const { RefCell::new(Vec::new()) };
}
struct MakeScope;
impl Drop for MakeScope {
    fn drop(&mut self) {
        MAKING.with(|making| {
            making.borrow_mut().pop();
        });
    }
}
pub(super) fn track_making_view(view: &View) {
    let host = MAKING.with(|making| making.borrow().last().and_then(Weak::upgrade));
    if let Some(host) = host
        && let Err(error) = Host(host).track(view)
    {
        view.report_error(error);
        view.close();
    }
}
impl Host {
    pub fn new(executor: Rc<dyn Executor>) -> Self {
        Self(Rc::new(HostInner {
            executor,
            session: RefCell::default(),
            slots: RefCell::default(),
            building: RefCell::default(),
            made: Cell::new(false),
            making: Cell::new(false),
            closed: Cell::new(false),
            error: RefCell::default(),
            close_timeout: Cell::new(Duration::from_secs(5)),
            changed: RefCell::default(),
            notifying: Cell::new(false),
            pending_notification: Cell::new(false),
        }))
    }
    pub fn error(&self) -> Option<String> {
        self.0.error.borrow().clone()
    }
    pub fn is_closed(&self) -> bool {
        self.0.closed.get()
    }
    pub fn session(&self) -> Option<Session> {
        self.0.session.borrow().clone()
    }
    pub fn set_close_timeout(&self, timeout: Duration) {
        self.0.close_timeout.set(timeout);
    }
    pub fn set_changed(&self, callback: Rc<dyn Fn()>) {
        let old = self.0.changed.replace(Some(callback));
        drop(old);
        self.0.notify();
    }
    pub fn registered_views(&self) -> Vec<(String, View)> {
        if self.0.making.get() {
            return Vec::new();
        }
        self.0
            .slots
            .borrow()
            .iter()
            .filter(|(_, slot)| slot.active.get() && !slot.view.is_closed())
            .map(|(id, slot)| (id.clone(), slot.view.clone()))
            .collect()
    }
    pub fn views(&self) -> Vec<View> {
        self.registered_views()
            .into_iter()
            .map(|(_, view)| view)
            .collect()
    }
    pub fn building_views(&self) -> Vec<View> {
        let mut views = self.views();
        let building = self.0.building.borrow().clone();
        for view in building.into_iter().filter(|view| !view.is_closed()) {
            if !views.iter().any(|existing| existing.same_view(&view)) {
                views.push(view);
            }
        }
        views
    }
    fn check_open(&self) -> Result<(), String> {
        if let Some(error) = self.error() {
            return Err(error);
        }
        if self.is_closed() {
            return Err("host is closed".into());
        }
        Ok(())
    }
    fn check_session(&self, session: &Session) -> Result<(), String> {
        self.check_open()?;
        if session.shutdown().phase() != ShutdownPhase::Running {
            return Err("cannot attach/register a shutting down Session".into());
        }
        if self
            .session()
            .is_some_and(|attached| !attached.ptr_eq(session))
        {
            return Err("foreign Session (Host accepts one shared Session)".into());
        }
        Ok(())
    }
    fn check_view(&self, view: &View) -> Result<(), String> {
        self.check_open()?;
        if view.is_closed() {
            return Err("cannot register a closed View".into());
        }
        if let Some(owner) = view.0.host.borrow().as_ref()
            && !owner
                .upgrade()
                .is_some_and(|owner| Rc::ptr_eq(&owner, &self.0))
        {
            return Err("foreign-host View".into());
        }
        Ok(())
    }
    fn track(&self, view: &View) -> Result<(), String> {
        self.check_view(view)?;
        if view.0.host.borrow().is_none() {
            view.0.host.replace(Some(Rc::downgrade(&self.0)));
        }
        if !self
            .0
            .building
            .borrow()
            .iter()
            .any(|existing| existing.same_view(view))
            && !self.views().iter().any(|existing| existing.same_view(view))
        {
            self.0.building.borrow_mut().push(view.clone());
        }
        Ok(())
    }
    pub fn new_view(&self, title: Binding<String>) -> Result<View, String> {
        self.check_open()?;
        let view = View::new(title);
        self.track(&view)?;
        Ok(view)
    }
    pub fn make(&self, make: impl FnOnce(Host) -> Result<(), String>) -> Result<(), String> {
        self.check_open()?;
        if self.0.made.replace(true) {
            return Err("make already called".into());
        }
        self.0.making.set(true);
        MAKING.with(|making| making.borrow_mut().push(Rc::downgrade(&self.0)));
        let scope = MakeScope;
        let result = catch_unwind(AssertUnwindSafe(|| make(self.clone())));
        drop(scope);
        self.0.making.set(false);
        let result = match result {
            Ok(result) => result,
            Err(panic) => Err(format!("make panicked: {}", panic_message(&*panic))),
        };
        if let Err(error) = result {
            self.fail(error);
            return Err(self.error().expect("make error"));
        }
        if let Some(error) = self.error() {
            return Err(error);
        }
        if let Some(error) = self.building_views().iter().find_map(ViewCore::error) {
            self.fail(error);
            return Err(self.error().expect("view error"));
        }
        for view in self.views() {
            view.present();
        }
        self.0.notify();
        self.check_open()
    }
    pub fn register_view(&self, session: &Session, id: &str, view: &View) -> Result<(), String> {
        <Self as WindowHost<View>>::register_view(self, session, id, view)
    }
    pub fn register_view_with_hooks(
        &self,
        session: &Session,
        id: &str,
        view: &View,
        hooks: WindowHooks,
    ) -> Result<(), String> {
        <Self as WindowHost<View>>::register_view_with_hooks(self, session, id, view, hooks)
    }
    fn remove(&self, id: &str, expected: &Weak<Slot>) {
        let slot = {
            let mut slots = self.0.slots.borrow_mut();
            if slots
                .get(id)
                .is_some_and(|slot| Weak::ptr_eq(&Rc::downgrade(slot), expected))
            {
                slots.remove(id)
            } else {
                None
            }
        };
        if let Some(slot) = slot {
            slot.retire();
            drop(slot);
            self.0.notify();
        }
    }
    pub fn request_window_close(&self, id: &str) -> Result<(), String> {
        self.check_open()?;
        let session = self.session().ok_or("no attached Session")?;
        if session.shutdown().phase() != ShutdownPhase::Running {
            return Ok(());
        }
        let slot = self
            .0
            .slots
            .borrow()
            .get(id)
            .cloned()
            .ok_or_else(|| format!("unknown window: {id}"))?;
        if !slot.active.get() || slot.view.is_closed() {
            return Err("window is closed".into());
        }
        if slot.requesting.replace(true) {
            return Ok(());
        }
        let (abort, registration) = AbortHandle::new_pair();
        let old = slot.abort.replace(Some(abort));
        drop(old);
        let host = Rc::downgrade(&self.0);
        let target = Rc::downgrade(&slot);
        let timeout = self.0.close_timeout.get();
        let mut lease = RequestLease {
            host: host.clone(),
            slot: target.clone(),
            armed: true,
        };
        let future: LocalBoxFuture<'static, ()> = Box::pin(async move {
            let work = async {
                let Some(slot) = target.upgrade() else {
                    return;
                };
                if !(slot.hooks.confirm)().await || !slot.active.get() {
                    return;
                }
                if session.shutdown().phase() != ShutdownPhase::Running {
                    return;
                }
                (slot.hooks.cancel)();
                if !slot.active.get() || session.shutdown().phase() != ShutdownPhase::Running {
                    return;
                }
                let _ = select(
                    (slot.hooks.drain)(),
                    Box::pin(futures_timer::Delay::new(timeout)),
                )
                .await;
                if slot.active.get() && session.shutdown().phase() == ShutdownPhase::Running {
                    (slot.hooks.close)();
                }
            };
            let result = AssertUnwindSafe(Abortable::new(work, registration))
                .catch_unwind()
                .await;
            lease.armed = false;
            if let Some(slot) = lease.slot.upgrade() {
                slot.requesting.set(false);
                let old = slot.abort.take();
                drop(old);
            }
            if let Err(panic) = result
                && let Some(host) = host.upgrade()
            {
                Host(host).fail(format!("window close panicked: {}", panic_message(&*panic)));
            }
        });
        if let Err(panic) = catch_unwind(AssertUnwindSafe(|| self.0.executor.spawn(future))) {
            self.fail(format!(
                "executor spawn panicked: {}",
                panic_message(&*panic)
            ));
            return Err(self.error().expect("executor error"));
        }
        self.check_open()
    }
    pub fn fail(&self, error: String) {
        if self.0.error.borrow().is_none() {
            self.0.error.replace(Some(error));
        }
        self.close();
    }
    pub fn close(&self) {
        self.0.shutdown();
    }
}
struct RequestLease {
    host: Weak<HostInner>,
    slot: Weak<Slot>,
    armed: bool,
}
impl Drop for RequestLease {
    fn drop(&mut self) {
        if self.armed
            && self
                .slot
                .upgrade()
                .is_some_and(|slot| slot.active.get() && slot.requesting.get())
            && let Some(host) = self.host.upgrade()
        {
            Host(host).fail("executor discarded an accepted close continuation".into());
        }
    }
}
impl HostInner {
    fn notify(&self) {
        self.pending_notification.set(true);
        if self.making.get() {
            return;
        }
        if self.notifying.replace(true) {
            return;
        }
        for _ in 0..64 {
            self.pending_notification.set(false);
            let callback = self.changed.borrow().clone();
            if let Some(callback) = callback
                && let Err(panic) = catch_unwind(AssertUnwindSafe(|| callback()))
            {
                if self.error.borrow().is_none() {
                    self.error.replace(Some(format!(
                        "host notification panicked: {}",
                        panic_message(&*panic)
                    )));
                }
                self.shutdown();
                break;
            }
            if !self.pending_notification.get() {
                break;
            }
        }
        if self.pending_notification.replace(false) && !self.closed.get() {
            if self.error.borrow().is_none() {
                self.error
                    .replace(Some("host notification feedback limit".into()));
            }
            self.shutdown();
            self.pending_notification.set(false);
        }
        self.notifying.set(false);
    }
    fn shutdown(&self) {
        if self.closed.replace(true) {
            return;
        }
        let session = self.session.take();
        let slots = self.slots.take();
        let building = self.building.take();
        if let Some(session) = session {
            session.shutdown().close_now();
        }
        for slot in slots.into_values() {
            slot.view.close();
            slot.retire();
        }
        for view in building {
            view.close();
        }
        self.notify();
    }
}
impl Drop for HostInner {
    fn drop(&mut self) {
        self.shutdown();
    }
}
impl UiHost for Host {
    fn executor(&self) -> Rc<dyn Executor> {
        self.0.executor.clone()
    }
    fn attach(&self, session: Session) -> Result<(), String> {
        self.check_session(&session)?;
        if self.0.session.borrow().is_none() {
            self.0.session.replace(Some(session));
        }
        Ok(())
    }
}
impl WindowHost<View> for Host {
    fn register_view_with_hooks(
        &self,
        session: &Session,
        id: &str,
        view: &View,
        hooks: WindowHooks,
    ) -> Result<(), String> {
        self.check_session(session)?;
        self.check_view(view)?;
        if id.is_empty() {
            return Err("empty window id".into());
        }
        {
            let slots = self.0.slots.borrow();
            if slots.contains_key(id) {
                return Err(format!("duplicate window: {id}"));
            }
            if slots.values().any(|slot| slot.view.same_view(view)) {
                return Err("View already registered under another id".into());
            }
        }
        let cancelled = Rc::new(Cell::new(false));
        let cancel = hooks.cancel.clone();
        let closed = Rc::new(Cell::new(false));
        let final_close = hooks.close.clone();
        let weak_view = view.downgrade();
        let weak_host = Rc::downgrade(&self.0);
        let hooks = WindowHooks {
            confirm: hooks.confirm,
            drain: hooks.drain,
            cancel: Rc::new(move || {
                if !cancelled.replace(true) {
                    cancel();
                }
            }),
            close: Rc::new(move || {
                if closed.replace(true) {
                    return;
                }
                let result = catch_unwind(AssertUnwindSafe(|| final_close()));
                if let Some(view) = weak_view.upgrade() {
                    view.close();
                }
                if let Err(panic) = result
                    && let Some(host) = weak_host.upgrade()
                {
                    Host(host).fail(format!(
                        "supplied close hook panicked: {}",
                        panic_message(&*panic)
                    ));
                }
            }),
        };
        let token = session.register_window(id, hooks.clone())?;
        if self.0.session.borrow().is_none() {
            self.0.session.replace(Some(session.clone()));
        }
        if view.0.host.borrow().is_none() {
            view.0.host.replace(Some(Rc::downgrade(&self.0)));
        }
        let slot = Rc::new(Slot {
            view: view.clone(),
            hooks,
            registration: RefCell::new(Some(token)),
            active: Cell::new(true),
            requesting: Cell::new(false),
            abort: RefCell::default(),
        });
        let partial = {
            let mut building = self.0.building.borrow_mut();
            building
                .iter()
                .position(|existing| existing.same_view(view))
                .map(|index| building.remove(index))
        };
        drop(partial);
        self.0.slots.borrow_mut().insert(id.into(), slot.clone());
        let owner = Rc::downgrade(&self.0);
        let target = Rc::downgrade(&slot);
        let key = id.to_owned();
        view.on_close(Rc::new(move || {
            if let Some(host) = owner.upgrade() {
                Host(host).remove(&key, &target);
            }
        }));
        let owner = Rc::downgrade(&self.0);
        let key = id.to_owned();
        view.set_host_request(Rc::new(move || {
            if let Some(host) = owner.upgrade() {
                let host = Host(host);
                if let Err(error) = host.request_window_close(&key) {
                    host.fail(error);
                }
            }
        }));
        if !self.0.making.get() {
            view.present();
        }
        self.0.notify();
        self.check_open()
    }
}
