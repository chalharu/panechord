use panechord_runtime::{Binding, Command, Executor, Layout, Store, Style};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};

#[derive(Clone)]
pub struct Model {
    pub name: String,
    pub flag: bool,
    pub items: Vec<panechord_runtime::MenuItem<i32>>,
}

#[panechord_macros::commands]
impl Model {
    #[command]
    fn reset(&mut self) {
        self.name.clear();
    }
    #[command]
    fn typed(&mut self, value: i32) {
        self.name = value.to_string();
    }
}

struct NoTasks;
impl Executor for NoTasks {
    fn spawn(&self, _: Pin<Box<dyn Future<Output = ()>>>) {
        panic!("these fixtures must not spawn tasks");
    }
}

pub fn model() -> Store<Model> {
    Store::new(
        Model {
            name: "model text".into(),
            flag: true,
            items: vec![panechord_runtime::MenuItem {
                id: "integer".into(),
                label: "Integer".into(),
                argument: 42,
                shortcut: None,
                role: panechord_runtime::MenuRole::Normal,
                enabled: true,
                visible: true,
            }],
        },
        Rc::new(NoTasks),
    )
}

// Backend-free construction probe shared by the macro test suites.
// `integration.rs` includes this file (`#[path] mod support`) instead of
// keeping a second copy: the trait boilerplate below is identical, only
// the recording interior differs, and the richer `State` version here is
// the canonical one. Template mounting is eager for the requested initial
// viewport; production recycling/scroll behavior belongs to backend tests.
//
// Resolution note: the inherent `typed_menu` below records `String`
// menus, so generated `String`-menu code binds to it; every other
// argument type falls through to the `MenuView` trait method, which
// dispatches (this is what the `typed-menu-ok` fixture probe relies on).
// Only the methods emitted by these fixtures; no GTK/Tao or permissive generics.
// The shared capability traits are implemented so the generated screen bounds
// on abstractions (`Backend::View` + the `View` contract), exactly like a real
// backend. Stubs are never called by these fixtures; they exist to satisfy the
// contract a new backend implements.
pub mod fake {
    use super::*;
    use panechord_runtime::{
        AutomationView, Backend as BackendTrait, ButtonView, Choice, DialogFooter, DialogView,
        ImageFrame, Keyed, LayoutView, ListView, MediaView, MenuItem, MenuNode, MenuView,
        PointerPosition, SelectView, Session, SliderRange, Subscription, TabPosition, TabView,
        Table, TableSelection, TableView, TextView, UiHost, ViewCore, Viewport, WeakViewHandle,
        WindowHooks, WindowHost,
    };
    use std::collections::BTreeMap;

    type Template<T> = Rc<dyn Fn(&View, &Parent, &str, Store<T>)>;

    #[derive(Clone, Default)]
    pub struct Parent;

    #[derive(Default)]
    pub struct State {
        pub title: Option<Binding<String>>,
        pub texts: BTreeMap<String, Binding<String>>,
        pub commands: BTreeMap<String, Command>,
        pub values: BTreeMap<String, Binding<f64>>,
        pub viewports: BTreeMap<String, Binding<Viewport>>,
        pub layouts: BTreeMap<String, Layout>,
        pub styles: BTreeMap<String, Style>,
        pub menus: BTreeMap<String, Binding<Vec<MenuItem>>>,
        pub dispatch: BTreeMap<String, Rc<dyn Fn(String) -> bool>>,
        pub tokens: Vec<Subscription>,
        pub close: Option<Rc<dyn Fn()>>,
        /// Last label text, mirroring the original single-string fixture:
        /// `label()` overwrites it, `entry()`/`button()` only record into
        /// `texts`. The `pass` fixture asserts the exact executable output.
        pub output: String,
    }

    #[derive(Clone, Default)]
    pub struct View(pub Rc<RefCell<State>>);

    impl View {
        pub fn try_layout(
            &self,
            parent: &Parent,
            id: &str,
            value: Layout,
        ) -> Result<Parent, String> {
            Ok(self.layout(parent, id, value))
        }
        pub fn report_error(&self, error: String) {
            panic!("{error}");
        }
        pub fn new(title: Binding<String>) -> Self {
            let v = Self::default();
            v.0.borrow_mut().title = Some(title);
            v
        }
        pub fn root(&self) -> Parent {
            Parent
        }
        pub fn layout(&self, _: &Parent, id: &str, value: Layout) -> Parent {
            assert!(
                self.0
                    .borrow_mut()
                    .layouts
                    .insert(id.into(), value)
                    .is_none()
            );
            Parent
        }
        pub fn style(&self, id: &str, value: Style) {
            self.0.borrow_mut().styles.insert(id.into(), value);
        }
        pub fn label(&self, _: &Parent, id: &str, text: Binding<String>) {
            let rendered = text.get();
            assert!(self.0.borrow_mut().texts.insert(id.into(), text).is_none());
            self.0.borrow_mut().output = rendered;
        }
        pub fn entry(&self, _: &Parent, id: &str, text: Binding<String>) {
            assert!(self.0.borrow_mut().texts.insert(id.into(), text).is_none());
        }
        pub fn button(&self, _: &Parent, id: &str, text: Binding<String>, command: Command) {
            assert!(self.0.borrow_mut().texts.insert(id.into(), text).is_none());
            self.0.borrow_mut().commands.insert(id.into(), command);
        }
        pub fn output(&self) -> String {
            self.0.borrow().output.clone()
        }
        pub fn meter(&self, _: &Parent, id: &str, value: Binding<f64>) {
            self.0.borrow_mut().values.insert(id.into(), value);
        }
        pub fn items<T: Clone + 'static>(
            &self,
            p: &Parent,
            id: &str,
            items: Binding<Vec<Keyed<T>>>,
            viewport: Binding<Viewport>,
            template: Template<T>,
        ) {
            let vp = viewport.get();
            self.0.borrow_mut().viewports.insert(id.into(), viewport);
            for item in items.get().into_iter().skip(vp.start).take(vp.count) {
                template(self, p, &format!("{id}/{}", item.key), item.model);
            }
        }
        pub fn content<T: Clone + 'static>(
            &self,
            p: &Parent,
            id: &str,
            item: Binding<Option<Keyed<T>>>,
            template: Template<T>,
        ) {
            if let Some(item) = item.get() {
                template(self, p, &format!("{id}/{}", item.key), item.model);
            }
        }
        pub fn tab_items<T: Clone + 'static>(
            &self,
            parent: &Parent,
            id: &str,
            items: Binding<Vec<Keyed<T>>>,
            _: Binding<String>,
            title: Rc<dyn Fn(Store<T>) -> Binding<String>>,
            template: Template<T>,
        ) -> Parent {
            for item in items.get() {
                let scope = format!("{id}/{}", item.key);
                self.label(parent, &scope, title(item.model.clone()));
                template(self, parent, &scope, item.model);
            }
            Parent
        }
        pub fn tab_position(&self, _: &str, _: TabPosition) -> Result<(), String> {
            Ok(())
        }
        /// Generic over the menu argument so fixtures with `MenuItem<i32>`
        /// (see the `typed-menu-ok` fixture probe) bind here instead of the
        /// `MenuView` trait method. `String` menus additionally record into
        /// `menus`/`dispatch` for the integration assertions; every argument
        /// type dispatches live items.
        pub fn typed_menu<A: Clone + 'static>(
            &self,
            id: &str,
            _title: Binding<String>,
            items: Binding<Vec<MenuItem<A>>>,
            dispatch: Rc<dyn Fn(A) -> bool>,
        ) {
            use std::any::Any;
            if let (Some(string_items), Some(string_dispatch)) = (
                (&items as &dyn Any).downcast_ref::<Binding<Vec<MenuItem<String>>>>(),
                (&dispatch as &dyn Any).downcast_ref::<Rc<dyn Fn(String) -> bool>>(),
            ) {
                self.0
                    .borrow_mut()
                    .menus
                    .insert(id.into(), string_items.clone());
                self.0
                    .borrow_mut()
                    .dispatch
                    .insert(id.into(), string_dispatch.clone());
            }
            for item in items.get() {
                dispatch(item.argument);
            }
        }
        pub fn keep(&self, token: Subscription) {
            self.0.borrow_mut().tokens.push(token);
        }
        pub fn on_close(&self, close: Rc<dyn Fn()>) {
            self.0.borrow_mut().close = Some(close);
        }
        pub fn close(&self) {
            let close = self.0.borrow_mut().close.take();
            let tokens = std::mem::take(&mut self.0.borrow_mut().tokens);
            self.0.borrow_mut().dispatch.clear();
            drop(tokens);
            if let Some(close) = close {
                close();
            }
        }
    }

    // Shared-contract implementation: the generated screen bounds on
    // abstractions (`Backend::View` + the `View` contract), exactly like a
    // real backend. Inherent methods above keep their shape; each trait
    // method delegates to the inherent one where the fixture records state,
    // and stubs the rest (never called by these fixtures).
    #[derive(Clone)]
    pub struct Weak(std::rc::Weak<RefCell<State>>);
    impl WeakViewHandle for Weak {
        type View = View;
        fn upgrade(&self) -> Option<View> {
            self.0.upgrade().map(View)
        }
    }

    impl ViewCore for View {
        type Parent = Parent;
        type Weak = Weak;
        fn new(title: Binding<String>) -> Self {
            View::new(title)
        }
        fn root(&self) -> Parent {
            View::root(self)
        }
        fn title(&self) -> String {
            self.0
                .borrow()
                .title
                .clone()
                .map(|title| title.get())
                .unwrap_or_default()
        }
        fn present(&self) {}
        fn close(&self) {
            View::close(self)
        }
        fn request_close(&self) {
            View::close(self)
        }
        fn on_close(&self, close: Rc<dyn Fn()>) {
            View::on_close(self, close)
        }
        fn on_close_request(&self, _: Rc<dyn Fn()>) {}
        fn downgrade(&self) -> Weak {
            Weak(Rc::downgrade(&self.0))
        }
        fn keep(&self, token: Subscription) {
            View::keep(self, token)
        }
        fn visible(&self, _: &str, _: Binding<bool>) {}
        fn enabled(&self, _: &str, _: Binding<bool>) {}
        fn style_binding(&self, _: &str, _: Binding<Style>) {}
        fn error(&self) -> Option<String> {
            None
        }
        fn report_error(&self, error: String) {
            View::report_error(self, error)
        }
    }

    impl LayoutView for View {
        fn layout(&self, parent: &Parent, id: &str, value: Layout) -> Parent {
            View::layout(self, parent, id, value)
        }
        fn try_layout(&self, parent: &Parent, id: &str, value: Layout) -> Result<Parent, String> {
            View::try_layout(self, parent, id, value)
        }
        fn try_scroll(&self, _: &Parent, _: &str, _: bool) -> Result<Parent, String> {
            Ok(Parent)
        }
        fn style(&self, id: &str, value: Style) {
            View::style(self, id, value)
        }
        fn try_style(&self, id: &str, value: Style) -> Result<(), String> {
            View::style(self, id, value);
            Ok(())
        }
        fn separator(&self, _: &Parent, _: &str, _: bool) {}
    }

    impl TextView for View {
        fn label(&self, parent: &Parent, id: &str, text: Binding<String>) {
            View::label(self, parent, id, text)
        }
        fn entry(&self, parent: &Parent, id: &str, text: Binding<String>) {
            View::entry(self, parent, id, text)
        }
        fn entry_with_placeholder(
            &self,
            parent: &Parent,
            id: &str,
            text: Binding<String>,
            _: Binding<String>,
        ) {
            View::entry(self, parent, id, text)
        }
    }

    impl ButtonView for View {
        fn button(&self, parent: &Parent, id: &str, text: Binding<String>, command: Command) {
            View::button(self, parent, id, text, command)
        }
        fn dialog_footer(&self, _: &Parent, _: &str, _: DialogFooter) {}
    }

    impl SelectView for View {
        fn checkbox(&self, _: &Parent, _: &str, _: Binding<String>, _: Binding<bool>) {}
        fn radio(&self, _: &Parent, _: &str, _: Binding<String>, _: Binding<String>, _: &str) {}
        fn slider(&self, _: &Parent, _: &str, _: Binding<f64>, _: SliderRange) {}
        fn combo(&self, _: &Parent, _: &str, _: Binding<Vec<Choice>>, _: Binding<String>) {}
        fn slider_value(&self, _: &str) -> Option<f64> {
            None
        }
    }

    impl MediaView for View {
        fn image(&self, _: &Parent, _: &str, _: Binding<Option<ImageFrame>>) {}
        fn image_with_pointer(
            &self,
            _: &Parent,
            _: &str,
            _: Binding<Option<ImageFrame>>,
            _: Binding<Option<PointerPosition>>,
        ) {
        }
        fn image_update_count(&self) -> usize {
            0
        }
        fn meter(&self, parent: &Parent, id: &str, value: Binding<f64>) {
            View::meter(self, parent, id, value)
        }
    }

    impl TabView for View {
        fn tab(&self, _: &Parent, _: &str, _: Binding<String>) -> Parent {
            Parent
        }
        fn navigation(&self, _: &Parent, _: &str, _: Binding<String>) -> Parent {
            Parent
        }
        fn tab_position(&self, id: &str, position: TabPosition) -> Result<(), String> {
            View::tab_position(self, id, position)
        }
        fn tab_page(&self, _: &Parent, _: &str, _: Binding<String>, _: &str) -> Parent {
            Parent
        }
        fn try_tab_page(
            &self,
            _: &Parent,
            _: &str,
            _: Binding<String>,
            _: &str,
        ) -> Result<Parent, String> {
            Ok(Parent)
        }
        fn finish_tab(&self, _: &Parent) -> Result<(), String> {
            Ok(())
        }
        fn tab_items<T: Clone + 'static>(
            &self,
            parent: &Parent,
            id: &str,
            items: Binding<Vec<Keyed<T>>>,
            selected: Binding<String>,
            title: Rc<dyn Fn(Store<T>) -> Binding<String>>,
            template: Rc<dyn Fn(&View, &Parent, &str, Store<T>)>,
        ) -> Parent {
            View::tab_items(self, parent, id, items, selected, title, template)
        }
    }

    impl ListView for View {
        fn repeat<T: Clone + 'static>(
            &self,
            parent: &Parent,
            id: &str,
            items: Binding<Vec<Keyed<T>>>,
            _: f32,
            template: Template<T>,
        ) {
            for item in items.get() {
                template(self, parent, &format!("{id}/{}", item.key), item.model);
            }
        }
        fn items<T: Clone + 'static>(
            &self,
            parent: &Parent,
            id: &str,
            items: Binding<Vec<Keyed<T>>>,
            viewport: Binding<Viewport>,
            template: Template<T>,
        ) {
            View::items(self, parent, id, items, viewport, template)
        }
        fn content<T: Clone + 'static>(
            &self,
            parent: &Parent,
            id: &str,
            item: Binding<Option<Keyed<T>>>,
            template: Template<T>,
        ) {
            View::content(self, parent, id, item, template)
        }
        fn item_keys(&self, _: &str) -> Vec<String> {
            Vec::new()
        }
        fn mounted_count(&self, _: &str) -> usize {
            0
        }
        fn viewport(&self, _: &str, _: usize) {}
        fn items_error(&self, _: &str) -> Option<String> {
            None
        }
    }

    impl DialogView for View {
        fn dialog(&self, _: &Parent, _: &str, _: Binding<String>, _: Binding<bool>) -> Parent {
            Parent
        }
    }

    impl TableView for View {
        fn table(&self, _: &Parent, _: &str, _: Binding<Table>) {}
        fn table_with_selection(
            &self,
            _: &Parent,
            _: &str,
            _: Binding<Table>,
            _: Binding<Option<TableSelection>>,
        ) {
        }
    }

    impl MenuView for View {
        fn typed_menu<A: Clone + 'static>(
            &self,
            _: &str,
            _: Binding<String>,
            items: Binding<Vec<MenuItem<A>>>,
            dispatch: Rc<dyn Fn(A) -> bool>,
        ) {
            for item in items.get() {
                dispatch(item.argument);
            }
        }
        fn hierarchical_menu<A: Clone + PartialEq + 'static>(
            &self,
            _: &str,
            _: Binding<String>,
            _: Binding<Vec<MenuNode<A>>>,
            _: Rc<dyn Fn(A) -> bool>,
        ) {
        }
        fn activate_menu(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    impl AutomationView for View {
        fn edit(&self, _: &str, _: &str) -> Result<(), String> {
            Err("fixture has no automation".into())
        }
        fn click(&self, _: &str) -> Result<(), String> {
            Err("fixture has no automation".into())
        }
        fn toggle(&self, _: &str, _: bool) -> Result<(), String> {
            Err("fixture has no automation".into())
        }
        fn slide(&self, _: &str, _: f64) -> Result<(), String> {
            Err("fixture has no automation".into())
        }
        fn select(&self, _: &str, _: &str) -> Result<(), String> {
            Err("fixture has no automation".into())
        }
        fn set_open(&self, _: &str, _: bool) -> Result<(), String> {
            Err("fixture has no automation".into())
        }
        fn text(&self, id: &str) -> Option<String> {
            self.0.borrow().texts.get(id).map(|text| text.get())
        }
    }

    pub struct Host(Rc<dyn Executor>);
    impl UiHost for Host {
        fn executor(&self) -> Rc<dyn Executor> {
            self.0.clone()
        }
        fn attach(&self, _: Session) -> Result<(), String> {
            Ok(())
        }
    }
    impl WindowHost<View> for Host {
        fn register_view_with_hooks(
            &self,
            _: &Session,
            _: &str,
            _: &View,
            _: WindowHooks,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    pub struct Backend;
    impl BackendTrait for Backend {
        type View = View;
        type Host = Host;
        const NAME: &str = "fake";
        fn run_host(
            _: impl FnOnce(Rc<Host>) -> Result<(), String> + 'static,
        ) -> Result<(), String> {
            Err("fixture has no native loop".into())
        }
        fn set_allows_automatic_window_tabbing(_: bool) {}
        fn set_initial_window_size(_: u32, _: u32) {}
    }
}
