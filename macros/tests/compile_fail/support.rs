use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
use panechord_runtime::{Binding, Command, Executor, Layout, Store, Style};

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
            items: vec![panechord_runtime::MenuItem { id: "integer".into(), label: "Integer".into(), argument: 42, shortcut: None, role: panechord_runtime::MenuRole::Normal, enabled: true, visible: true }],
        },
        Rc::new(NoTasks),
    )
}

// Only the methods emitted by these fixtures; no GTK/Tao or permissive generics.
// The shared capability traits are implemented so the generated screen bounds
// on abstractions (`Backend::View` + the `View` contract), exactly like a real
// backend. Stubs are never called by these fixtures; they exist to satisfy the
// contract a new backend implements.
pub mod fake {
    use super::*;
    use panechord_runtime::{
        AutomationView, Backend as BackendTrait, ButtonView, Choice, ContainerView, DialogFooter,
        ImageFrame, Keyed, LayoutView, MediaView, MenuItem, MenuNode, MenuView, PointerPosition,
        RunnerConfig, SelectView, Session, SliderRange, Subscription, Table, TableSelection,
        TabPosition, TextView, UiHost, ViewCore, Viewport, WeakViewHandle, WindowHooks, WindowHost,
    };

    #[derive(Clone)]
    pub struct Parent;
    #[derive(Clone)]
    pub struct View(Rc<RefCell<String>>);
    #[derive(Clone)]
    pub struct Weak(std::rc::Weak<RefCell<String>>);

    impl WeakViewHandle for Weak {
        type View = View;
        fn upgrade(&self) -> Option<View> {
            self.0.upgrade().map(View)
        }
    }

    impl View {
        pub fn try_layout(&self, parent: &Parent, id: &str, layout: Layout) -> Result<Parent, String> {
            Ok(self.layout(parent, id, layout))
        }
        pub fn report_error(&self, error: String) { panic!("{error}"); }
        pub fn new(_: Binding<String>) -> Self {
            Self(Rc::new(RefCell::new(String::new())))
        }
        pub fn root(&self) -> Parent {
            Parent
        }
        pub fn style(&self, _: &str, _: Style) {}
        pub fn layout(&self, _: &Parent, _: &str, _: Layout) -> Parent {
            Parent
        }
        pub fn label(&self, _: &Parent, _: &str, text: Binding<String>) {
            *self.0.borrow_mut() = text.get();
        }
        pub fn entry(&self, _: &Parent, _: &str, _: Binding<String>) {}
        pub fn button(&self, _: &Parent, _: &str, _: Binding<String>, _: Command) {}
        pub fn on_close(&self, _: Rc<dyn Fn()>) {}
        pub fn typed_menu<A: Clone + 'static>(&self, _: &str, _: Binding<String>, items: Binding<Vec<panechord_runtime::MenuItem<A>>>, dispatch: Rc<dyn Fn(A) -> bool>) {
            for item in items.get() { dispatch(item.argument); }
        }
        pub fn output(&self) -> String {
            self.0.borrow().clone()
        }
    }

    impl ViewCore for View {
        type Parent = Parent;
        type Weak = Weak;
        fn new(title: Binding<String>) -> Self { View::new(title) }
        fn root(&self) -> Parent { View::root(self) }
        fn title(&self) -> String { self.0.borrow().clone() }
        fn present(&self) {}
        fn close(&self) {}
        fn request_close(&self) {}
        fn on_close(&self, callback: Rc<dyn Fn()>) { View::on_close(self, callback) }
        fn on_close_request(&self, _: Rc<dyn Fn()>) {}
        fn downgrade(&self) -> Weak { Weak(Rc::downgrade(&self.0)) }
        fn keep(&self, _: Subscription) {}
        fn visible(&self, _: &str, _: Binding<bool>) {}
        fn enabled(&self, _: &str, _: Binding<bool>) {}
        fn style_binding(&self, _: &str, _: Binding<Style>) {}
        fn error(&self) -> Option<String> { None }
        fn report_error(&self, error: String) { View::report_error(self, error) }
    }

    impl LayoutView for View {
        fn layout(&self, parent: &Parent, id: &str, layout: Layout) -> Parent {
            View::layout(self, parent, id, layout)
        }
        fn try_layout(&self, parent: &Parent, id: &str, layout: Layout) -> Result<Parent, String> {
            View::try_layout(self, parent, id, layout)
        }
        fn try_scroll(&self, _: &Parent, _: &str, _: bool) -> Result<Parent, String> { Ok(Parent) }
        fn style(&self, id: &str, style: Style) { View::style(self, id, style) }
        fn try_style(&self, id: &str, style: Style) -> Result<(), String> {
            View::style(self, id, style);
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
        fn entry_with_placeholder(&self, parent: &Parent, id: &str, text: Binding<String>, _: Binding<String>) {
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
        fn slider_value(&self, _: &str) -> Option<f64> { None }
    }

    impl MediaView for View {
        fn image(&self, _: &Parent, _: &str, _: Binding<Option<ImageFrame>>) {}
        fn image_with_pointer(&self, _: &Parent, _: &str, _: Binding<Option<ImageFrame>>, _: Binding<Option<PointerPosition>>) {}
        fn image_update_count(&self) -> usize { 0 }
        fn meter(&self, _: &Parent, _: &str, _: Binding<f64>) {}
    }

    impl ContainerView for View {
        fn tab(&self, _: &Parent, _: &str, _: Binding<String>) -> Parent { Parent }
        fn navigation(&self, _: &Parent, _: &str, _: Binding<String>) -> Parent { Parent }
        fn tab_position(&self, _: &str, _: TabPosition) -> Result<(), String> { Ok(()) }
        fn tab_page(&self, _: &Parent, _: &str, _: Binding<String>, _: &str) -> Parent { Parent }
        fn try_tab_page(&self, _: &Parent, _: &str, _: Binding<String>, _: &str) -> Result<Parent, String> { Ok(Parent) }
        fn finish_tab(&self, _: &Parent) -> Result<(), String> { Ok(()) }
        fn tab_items<T: Clone + 'static>(&self, _: &Parent, _: &str, _: Binding<Vec<Keyed<T>>>, _: Binding<String>, _: Rc<dyn Fn(Store<T>) -> Binding<String>>, _: Rc<dyn Fn(&View, &Parent, &str, Store<T>)>) -> Parent { Parent }
        fn repeat<T: Clone + 'static>(&self, _: &Parent, _: &str, _: Binding<Vec<Keyed<T>>>, _: f32, _: Rc<dyn Fn(&View, &Parent, &str, Store<T>)>) {}
        fn items<T: Clone + 'static>(&self, _: &Parent, _: &str, _: Binding<Vec<Keyed<T>>>, _: Binding<Viewport>, _: Rc<dyn Fn(&View, &Parent, &str, Store<T>)>) {}
        fn content<T: Clone + 'static>(&self, _: &Parent, _: &str, _: Binding<Option<Keyed<T>>>, _: Rc<dyn Fn(&View, &Parent, &str, Store<T>)>) {}
        fn dialog(&self, _: &Parent, _: &str, _: Binding<String>, _: Binding<bool>) -> Parent { Parent }
        fn table(&self, _: &Parent, _: &str, _: Binding<Table>) {}
        fn table_with_selection(&self, _: &Parent, _: &str, _: Binding<Table>, _: Binding<Option<TableSelection>>) {}
        fn item_keys(&self, _: &str) -> Vec<String> { Vec::new() }
        fn mounted_count(&self, _: &str) -> usize { 0 }
        fn viewport(&self, _: &str, _: usize) {}
        fn items_error(&self, _: &str) -> Option<String> { None }
    }

    impl MenuView for View {
        fn typed_menu<A: Clone + 'static>(&self, id: &str, title: Binding<String>, items: Binding<Vec<MenuItem<A>>>, dispatch: Rc<dyn Fn(A) -> bool>) {
            View::typed_menu(self, id, title, items, dispatch)
        }
        fn hierarchical_menu<A: Clone + PartialEq + 'static>(&self, _: &str, _: Binding<String>, _: Binding<Vec<MenuNode<A>>>, _: Rc<dyn Fn(A) -> bool>) {}
        fn activate_menu(&self, _: &str) -> Result<(), String> { Ok(()) }
    }

    impl AutomationView for View {
        fn edit(&self, _: &str, _: &str) -> Result<(), String> { Err("fixture has no automation".into()) }
        fn click(&self, _: &str) -> Result<(), String> { Err("fixture has no automation".into()) }
        fn toggle(&self, _: &str, _: bool) -> Result<(), String> { Err("fixture has no automation".into()) }
        fn slide(&self, _: &str, _: f64) -> Result<(), String> { Err("fixture has no automation".into()) }
        fn select(&self, _: &str, _: &str) -> Result<(), String> { Err("fixture has no automation".into()) }
        fn set_open(&self, _: &str, _: bool) -> Result<(), String> { Err("fixture has no automation".into()) }
        fn text(&self, _: &str) -> Option<String> { None }
    }

    pub struct Host;
    impl Executor for Host {
        fn spawn(&self, _: Pin<Box<dyn Future<Output = ()>>>) {
            panic!("these fixtures must not spawn tasks");
        }
    }
    impl UiHost for Host {
        fn executor(&self) -> Rc<dyn Executor> { Rc::new(NoTasks) }
        fn attach(&self, _: Session) -> Result<(), String> { Ok(()) }
    }
    impl WindowHost<View> for Host {
        fn register_view(&self, _: &Session, _: &str, _: &View, _: WindowHooks) -> Result<(), String> {
            Ok(())
        }
    }

    pub struct Backend;
    impl BackendTrait for Backend {
        type View = View;
        type Host = Host;
        fn runner_config() -> RunnerConfig {
            RunnerConfig { application_menu: false, menu_quit_in_file: true }
        }
        fn run_host(_: impl FnOnce(Rc<Host>) -> Result<(), String> + 'static) -> Result<(), String> {
            Err("fixture has no native loop".into())
        }
        fn set_allows_automatic_window_tabbing(_: bool) {}
        fn set_initial_window_size(_: u32, _: u32) {}
    }
}
