//! P0 ONLY: caption-based discovery is not a production window identity contract.
use std::{cell::{Cell, RefCell}, collections::HashMap, rc::{Rc, Weak}};
use windows_sys::Win32::{Foundation::*,
    UI::{Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass}, WindowsAndMessaging::*}};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

thread_local! {
    static GATES: RefCell<HashMap<usize, Weak<Gate>>> = RefCell::new(HashMap::new());
}
pub struct Gate { hwnd: HWND, allow: Cell<bool>, destroyed: Cell<bool>, request: Box<dyn Fn()> }
impl Gate {
    #[cfg(test)]
    pub fn attach(title: &str, request: impl Fn() + 'static) -> Result<Rc<Self>, String> {
        struct Search { title: Vec<u16>, matches: Vec<HWND> }
        unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> i32 {
            let search = unsafe { &mut *(data as *mut Search) };
            let mut title = vec![0; unsafe { GetWindowTextLengthW(hwnd) } as usize + 1];
            let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
            title.truncate(n as usize);
            if title == search.title { search.matches.push(hwnd); }
            1
        }
        let mut search = Search { title: title.encode_utf16().collect(), matches: Vec::new() };
        unsafe { EnumThreadWindows(GetCurrentThreadId(), Some(find), &mut search as *mut _ as LPARAM); }
        if search.matches.len() != 1 { return Err(format!("expected one HWND for {title}, found {}", search.matches.len())); }
        Self::from_hwnd(search.matches[0], request)
    }
    pub(super) fn from_hwnd(hwnd: HWND, request: impl Fn() + 'static) -> Result<Rc<Self>, String> {
        if unsafe { IsWindow(hwnd) } == 0
            || unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) } != unsafe { GetCurrentThreadId() } {
            return Err("invalid or foreign-thread HWND".into());
        }
        let existing = GATES.with(|all| all.borrow().get(&(hwnd as usize)).and_then(Weak::upgrade));
        if existing.is_some_and(|gate| !gate.destroyed.get()) {
            return Err("HWND already has a P0 close gate".into());
        }
        let gate = Rc::new(Self { hwnd, allow: Cell::new(false), destroyed: Cell::new(false), request: Box::new(request) });
        // A unique ID cannot replace another owner's subclass with the same procedure.
        let id = Rc::as_ptr(&gate) as usize;
        if unsafe { SetWindowSubclass(gate.hwnd, Some(subclass), id, id) } == 0 {
            return Err("SetWindowSubclass failed".into());
        }
        GATES.with(|all| all.borrow_mut().insert(hwnd as usize, Rc::downgrade(&gate)));
        Ok(gate)
    }
    pub fn approve(&self) { self.allow.set(true); }
    pub fn matches(&self, hwnd: HWND) -> bool { self.hwnd == hwnd && !self.destroyed.get() }
    pub fn alive(&self) -> bool { !self.destroyed.get() && unsafe { IsWindow(self.hwnd) != 0 } }
    pub fn request_native_close(&self) -> Result<(), String> { self.post(WM_CLOSE, 0) }
    pub fn request_system_close(&self) -> Result<(), String> { self.post(WM_SYSCOMMAND, SC_CLOSE as usize) }
    fn post(&self, message: u32, wp: usize) -> Result<(), String> {
        if !self.alive() { return Err("close gate is destroyed".into()); }
        if unsafe { PostMessageW(self.hwnd, message, wp, 0) } == 0 {
            return Err(format!("PostMessageW close rejected: {}", unsafe { GetLastError() }));
        }
        Ok(())
    }
    /// UI-thread fatal-error teardown, independent of both Reactor and Win32 queues.
    pub fn force_close(&self) -> Result<(), String> {
        self.approve();
        if self.alive() { unsafe { SendMessageW(self.hwnd, WM_CLOSE, 0, 0); } }
        Ok(())
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        GATES.with(|all| {
            let mut all = all.borrow_mut();
            if all.get(&(self.hwnd as usize)).is_some_and(|gate| std::ptr::eq(gate.as_ptr(), self)) {
                all.remove(&(self.hwnd as usize));
            }
        });
        if !self.destroyed.get() {
            unsafe { RemoveWindowSubclass(self.hwnd, Some(subclass), self as *const Self as usize); }
        }
    }
}
unsafe extern "system" fn subclass(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM,
    id: usize, data: usize) -> LRESULT {
    let gate = unsafe { &*(data as *const Gate) };
    if message == WM_CLOSE && !gate.allow.get() {
        // Never unwind through a native callback. Reject close even if dispatch panics.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (gate.request)())).is_err() {
            eprintln!("P0 close callback panicked; native close rejected");
        }
        return 0;
    }
    if message == WM_NCDESTROY {
        // Never inspect/mutate a later window that reuses this numeric HWND.
        gate.destroyed.set(true);
        unsafe { RemoveWindowSubclass(hwnd, Some(subclass), id); }
    }
    unsafe { DefSubclassProc(hwnd, message, wp, lp) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;
    #[test]
    fn wm_close_veto_preserves_same_hwnd() {
        let title = format!("Panechord P0 Win32 gate {}", unsafe { GetCurrentThreadId() });
        let text: Vec<u16> = title.encode_utf16().chain([0]).collect();
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe { CreateWindowExW(0, class.as_ptr(), text.as_ptr(), WS_OVERLAPPED,
            0, 0, 100, 100, null_mut(), null_mut(), null_mut(), std::ptr::null()) };
        assert!(!hwnd.is_null());
        let count = Rc::new(Cell::new(0)); let out = count.clone();
        let gate = Gate::attach(&title, move || out.set(out.get() + 1)).unwrap();
        assert!(Gate::from_hwnd(hwnd, || ()).is_err());
        unsafe { SendMessageW(hwnd, WM_CLOSE, 0, 0); }
        assert_eq!(count.get(), 1); assert!(gate.alive());
        gate.approve(); unsafe { SendMessageW(hwnd, WM_CLOSE, 0, 0); }
        assert!(!gate.alive());
    }
}
