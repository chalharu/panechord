//! Shared view capabilities: the Interface Segregation contract.
//!
//! Each backend (`panechord-gtk`, `panechord-tao`, `panechord-macos`) used to
//! expose one ~65-method `View` with a 23–31-field `Inner`. Every client —
//! from a label-only screen to the menu-heavy library — depended on the whole
//! surface, and high-level code (codegen, `Runner`) named concrete backend
//! types. That was four SOLID violations at once:
//!
//! * **SRP**: one object owned window lifecycle, layout registry, menus,
//!   virtualized lists, and every widget family.
//! * **ISP**: clients depended on methods they never call.
//! * **DIP**: codegen and facades depended on `gtk::View` / `tao::View` /
//!   `macos::View` concretions instead of abstractions.
//! * **LSP**: with no shared type, a backend `View` could not substitute for
//!   another in generic code.
//!
//! These traits fix the interface side. Each capability is its own trait so a
//! screen only bounds on what it uses (`fn show<V: TextView + ButtonView>`).
//! The umbrella [`View`] trait re-combines them for code that genuinely needs
//! the full contract (codegen output, `Runner`). Backends keep their concrete
//! `View`/`Parent` types and inherent extension methods (native mount points,
//! probes); the shared contract is what generic code bounds on.
//!
//! [`Backend`](crate::Backend) is the matching host-side abstraction: one
//! `run_host` / option-setter shape per backend, so adding a backend
//! implements the trait instead of editing the facade dispatch.
//!
//! Backend-specific extensions stay inherent methods on the concrete `View`
//! (GTK's `mount_native`/`bind_native`, Tao's `mount`/`mount_bound`,
//! AppKit's `surface`/probes): that is the Open/Closed extension point. New
//! controls land there first without touching this contract; promotion to the
//! shared traits happens only once every backend speaks them.

use crate::{
    Binding, Choice, Command, DialogFooter, ImageFrame, Keyed, Layout, MenuItem, MenuNode,
    PointerPosition, SliderRange, Store, Style, Subscription, TabPosition, Table, TableSelection,
    Viewport,
};
use std::rc::Rc;

/// Weak handle counterpart of [`ViewCore`]. Backends implement this on their
/// concrete `WeakView`; generic teardown code bounds on it instead of naming
/// `gtk::WeakView` / `tao::WeakView` / `macos::WeakView`.
pub trait WeakViewHandle: Clone + 'static {
    /// The strong view this handle upgrades to.
    type View;
    /// Returns `None` once the view is closed or dropped.
    fn upgrade(&self) -> Option<Self::View>;
}

/// Construction, lifecycle, and diagnostics. Every screen needs this; nothing
/// here knows about widgets, layouts, or menus.
pub trait ViewCore: Clone + 'static {
    /// Backend mount-point handle.
    type Parent: Clone + 'static;
    /// Non-owning handle for teardown callbacks.
    type Weak: WeakViewHandle<View = Self>;

    fn new(title: Binding<String>) -> Self;
    fn root(&self) -> Self::Parent;
    fn title(&self) -> String;
    fn present(&self);
    fn close(&self);
    fn request_close(&self);
    fn on_close(&self, callback: Rc<dyn Fn()>);
    fn on_close_request(&self, callback: Rc<dyn Fn()>);
    fn downgrade(&self) -> Self::Weak;
    fn keep(&self, token: Subscription);
    /// Per-control applicability: every control accepts optional OneWay
    /// `visible`, `enabled` and `style_binding`. Visibility retains the
    /// mounted control; `style_binding` is a full `Style` replacement.
    fn visible(&self, id: &str, binding: Binding<bool>);
    fn enabled(&self, id: &str, binding: Binding<bool>);
    fn style_binding(&self, id: &str, binding: Binding<Style>);
    fn error(&self) -> Option<String>;
    fn report_error(&self, error: String);
}

/// Container geometry: layouts, scrolling, and spacing/placement validation.
pub trait LayoutView: ViewCore {
    fn layout(&self, parent: &Self::Parent, id: &str, layout: Layout) -> Self::Parent;
    fn try_layout(
        &self,
        parent: &Self::Parent,
        id: &str,
        layout: Layout,
    ) -> Result<Self::Parent, String>;
    fn try_scroll(
        &self,
        parent: &Self::Parent,
        id: &str,
        horizontal: bool,
    ) -> Result<Self::Parent, String>;
    fn style(&self, id: &str, style: Style);
    fn try_style(&self, id: &str, style: Style) -> Result<(), String>;
    /// Structural chrome shared by every backend.
    fn separator(&self, parent: &Self::Parent, id: &str, vertical: bool);
}

/// Single-line text display and editing.
pub trait TextView: ViewCore {
    fn label(&self, parent: &Self::Parent, id: &str, text: Binding<String>);
    fn entry(&self, parent: &Self::Parent, id: &str, text: Binding<String>);
    fn entry_with_placeholder(
        &self,
        parent: &Self::Parent,
        id: &str,
        text: Binding<String>,
        placeholder: Binding<String>,
    );
}

/// Push buttons and the framework-level dialog footer.
pub trait ButtonView: ViewCore {
    fn button(&self, parent: &Self::Parent, id: &str, text: Binding<String>, command: Command);
    fn dialog_footer(&self, parent: &Self::Parent, id: &str, footer: DialogFooter);
}

/// Stateful selection controls.
pub trait SelectView: ViewCore {
    fn checkbox(
        &self,
        parent: &Self::Parent,
        id: &str,
        text: Binding<String>,
        checked: Binding<bool>,
    );
    fn radio(
        &self,
        parent: &Self::Parent,
        id: &str,
        text: Binding<String>,
        selected: Binding<String>,
        value: &str,
    );
    fn slider(&self, parent: &Self::Parent, id: &str, value: Binding<f64>, range: SliderRange);
    fn combo(
        &self,
        parent: &Self::Parent,
        id: &str,
        items: Binding<Vec<Choice>>,
        selected: Binding<String>,
    );
    fn slider_value(&self, id: &str) -> Option<f64>;
}

/// Images and meters.
pub trait MediaView: ViewCore {
    fn image(&self, parent: &Self::Parent, id: &str, source: Binding<Option<ImageFrame>>);
    fn image_with_pointer(
        &self,
        parent: &Self::Parent,
        id: &str,
        source: Binding<Option<ImageFrame>>,
        pointer: Binding<Option<PointerPosition>>,
    );
    fn image_update_count(&self) -> usize;
    fn meter(&self, parent: &Self::Parent, id: &str, value: Binding<f64>);
}

/// Tab sets and navigation.
pub trait TabView: ViewCore {
    fn tab(&self, parent: &Self::Parent, id: &str, selected: Binding<String>) -> Self::Parent;
    fn navigation(
        &self,
        parent: &Self::Parent,
        id: &str,
        selected: Binding<String>,
    ) -> Self::Parent;
    fn tab_position(&self, id: &str, position: TabPosition) -> Result<(), String>;
    fn tab_page(
        &self,
        parent: &Self::Parent,
        id: &str,
        title: Binding<String>,
        key: &str,
    ) -> Self::Parent;
    fn try_tab_page(
        &self,
        parent: &Self::Parent,
        id: &str,
        title: Binding<String>,
        key: &str,
    ) -> Result<Self::Parent, String>;
    fn finish_tab(&self, parent: &Self::Parent) -> Result<(), String>;
    fn tab_items<T: Clone + 'static>(
        &self,
        parent: &Self::Parent,
        id: &str,
        items: Binding<Vec<Keyed<T>>>,
        selected: Binding<String>,
        title: Rc<dyn Fn(Store<T>) -> Binding<String>>,
        template: Template<Self, Self::Parent, T>,
    ) -> Self::Parent;
}

/// Virtualized lists, content projection, and their probes.
pub trait ListView: ViewCore {
    fn repeat<T: Clone + 'static>(
        &self,
        parent: &Self::Parent,
        id: &str,
        items: Binding<Vec<Keyed<T>>>,
        gap: f32,
        template: Template<Self, Self::Parent, T>,
    );
    fn items<T: Clone + 'static>(
        &self,
        parent: &Self::Parent,
        id: &str,
        items: Binding<Vec<Keyed<T>>>,
        viewport: Binding<Viewport>,
        template: Template<Self, Self::Parent, T>,
    );
    fn content<T: Clone + 'static>(
        &self,
        parent: &Self::Parent,
        id: &str,
        item: Binding<Option<Keyed<T>>>,
        template: Template<Self, Self::Parent, T>,
    );
    fn item_keys(&self, id: &str) -> Vec<String>;
    fn mounted_count(&self, id: &str) -> usize;
    fn viewport(&self, id: &str, start: usize);
    fn items_error(&self, id: &str) -> Option<String>;
}

/// Tables, with and without selection.
pub trait TableView: ViewCore {
    fn table(&self, parent: &Self::Parent, id: &str, data: Binding<Table>);
    fn table_with_selection(
        &self,
        parent: &Self::Parent,
        id: &str,
        data: Binding<Table>,
        selection: Binding<Option<TableSelection>>,
    );
}

/// Dialog windows.
pub trait DialogView: ViewCore {
    fn dialog(
        &self,
        parent: &Self::Parent,
        id: &str,
        title: Binding<String>,
        open: Binding<bool>,
    ) -> Self::Parent;
}

/// Menus: typed dispatch plus hierarchical submenus.
pub trait MenuView: ViewCore {
    fn typed_menu<A: Clone + 'static>(
        &self,
        id: &str,
        title: Binding<String>,
        items: Binding<Vec<MenuItem<A>>>,
        dispatch: Rc<dyn Fn(A) -> bool>,
    );
    fn hierarchical_menu<A: Clone + PartialEq + 'static>(
        &self,
        id: &str,
        title: Binding<String>,
        items: Binding<Vec<MenuNode<A>>>,
        dispatch: Rc<dyn Fn(A) -> bool>,
    );
    fn activate_menu(&self, id: &str) -> Result<(), String>;
}

/// Headless test automation: the same messages widgets send, without
/// synthesizing OS input events.
pub trait AutomationView: ViewCore {
    fn edit(&self, id: &str, value: &str) -> Result<(), String>;
    fn click(&self, id: &str) -> Result<(), String>;
    fn toggle(&self, id: &str, value: bool) -> Result<(), String>;
    fn slide(&self, id: &str, value: f64) -> Result<(), String>;
    fn select(&self, id: &str, value: &str) -> Result<(), String>;
    fn set_open(&self, id: &str, value: bool) -> Result<(), String>;
    fn text(&self, id: &str) -> Option<String>;
}

/// Row/content template: builds one item's controls into a scoped parent.
/// Backends keep their own concrete alias; generic code uses this one.
pub type Template<V, P, T> = Rc<dyn Fn(&V, &P, &str, Store<T>)>;

/// The full shared contract: every capability at once. Generic code that
/// needs the whole screen (codegen output, `Runner`) bounds on this; code
/// that needs less bounds on the individual capability traits.
pub trait View:
    ViewCore
    + LayoutView
    + TextView
    + ButtonView
    + SelectView
    + MediaView
    + TabView
    + ListView
    + TableView
    + DialogView
    + MenuView
    + AutomationView
{
}

impl<T> View for T where
    T: ViewCore
        + LayoutView
        + TextView
        + ButtonView
        + SelectView
        + MediaView
        + TabView
        + ListView
        + TableView
        + DialogView
        + MenuView
        + AutomationView
{
}

/// Generate the shared-contract implementations for a backend's concrete view.
/// One invocation replaces a ~350-line delegation file: the twelve capability
/// traits (`ViewCore` through `AutomationView`) each delegate to the inherent
/// method of the same name, plus `WeakViewHandle` and the [`Backend`] impl.
/// The backends stay identical by construction; backend-only extensions remain
/// inherent methods (the Open/Closed extension point).
#[macro_export]
macro_rules! backend_view_impls {
    (
        $Backend:ident, $View:ident, $Parent:ident, $Weak:ident, $Host:ident, $name:literal,
        $run_host:path, $set_tabbing:path, $set_size:path
    ) => {
        impl $crate::WeakViewHandle for $Weak {
            type View = $View;
            fn upgrade(&self) -> Option<$View> {
                $Weak::upgrade(self)
            }
        }

        impl $crate::ViewCore for $View {
            type $Parent = $Parent;
            type Weak = $Weak;
            fn new(title: $crate::Binding<String>) -> Self {
                $View::new(title)
            }
            fn root(&self) -> $Parent {
                $View::root(self)
            }
            fn title(&self) -> String {
                $View::title(self)
            }
            fn present(&self) {
                $View::present(self)
            }
            fn close(&self) {
                $View::close(self)
            }
            fn request_close(&self) {
                $View::request_close(self)
            }
            fn on_close(&self, callback: ::std::rc::Rc<dyn Fn()>) {
                $View::on_close(self, callback)
            }
            fn on_close_request(&self, callback: ::std::rc::Rc<dyn Fn()>) {
                $View::on_close_request(self, callback)
            }
            fn downgrade(&self) -> $Weak {
                $View::downgrade(self)
            }
            fn keep(&self, token: $crate::Subscription) {
                $View::keep(self, token)
            }
            fn visible(&self, id: &str, binding: $crate::Binding<bool>) {
                $View::visible(self, id, binding)
            }
            fn enabled(&self, id: &str, binding: $crate::Binding<bool>) {
                $View::enabled(self, id, binding)
            }
            fn style_binding(&self, id: &str, binding: $crate::Binding<$crate::Style>) {
                $View::style_binding(self, id, binding)
            }
            fn error(&self) -> Option<String> {
                $View::error(self)
            }
            fn report_error(&self, error: String) {
                $View::report_error(self, error)
            }
        }

        impl $crate::LayoutView for $View {
            fn layout(&self, parent: &$Parent, id: &str, layout: $crate::Layout) -> $Parent {
                $View::layout(self, parent, id, layout)
            }
            fn try_layout(
                &self,
                parent: &$Parent,
                id: &str,
                layout: $crate::Layout,
            ) -> Result<$Parent, String> {
                $View::try_layout(self, parent, id, layout)
            }
            fn try_scroll(
                &self,
                parent: &$Parent,
                id: &str,
                horizontal: bool,
            ) -> Result<$Parent, String> {
                $View::try_scroll(self, parent, id, horizontal)
            }
            fn style(&self, id: &str, style: $crate::Style) {
                $View::style(self, id, style)
            }
            fn try_style(&self, id: &str, style: $crate::Style) -> Result<(), String> {
                $View::try_style(self, id, style)
            }
            fn separator(&self, parent: &$Parent, id: &str, vertical: bool) {
                $View::separator(self, parent, id, vertical)
            }
        }

        impl $crate::TextView for $View {
            fn label(&self, parent: &$Parent, id: &str, text: $crate::Binding<String>) {
                $View::label(self, parent, id, text)
            }
            fn entry(&self, parent: &$Parent, id: &str, text: $crate::Binding<String>) {
                $View::entry(self, parent, id, text)
            }
            fn entry_with_placeholder(
                &self,
                parent: &$Parent,
                id: &str,
                text: $crate::Binding<String>,
                placeholder: $crate::Binding<String>,
            ) {
                $View::entry_with_placeholder(self, parent, id, text, placeholder)
            }
        }

        impl $crate::ButtonView for $View {
            fn button(
                &self,
                parent: &$Parent,
                id: &str,
                text: $crate::Binding<String>,
                command: $crate::Command,
            ) {
                $View::button(self, parent, id, text, command)
            }
            fn dialog_footer(&self, parent: &$Parent, id: &str, footer: $crate::DialogFooter) {
                $View::dialog_footer(self, parent, id, footer)
            }
        }

        impl $crate::SelectView for $View {
            fn checkbox(
                &self,
                parent: &$Parent,
                id: &str,
                text: $crate::Binding<String>,
                checked: $crate::Binding<bool>,
            ) {
                $View::checkbox(self, parent, id, text, checked)
            }
            fn radio(
                &self,
                parent: &$Parent,
                id: &str,
                text: $crate::Binding<String>,
                selected: $crate::Binding<String>,
                value: &str,
            ) {
                $View::radio(self, parent, id, text, selected, value)
            }
            fn slider(
                &self,
                parent: &$Parent,
                id: &str,
                value: $crate::Binding<f64>,
                range: $crate::SliderRange,
            ) {
                $View::slider(self, parent, id, value, range)
            }
            fn combo(
                &self,
                parent: &$Parent,
                id: &str,
                items: $crate::Binding<Vec<$crate::Choice>>,
                selected: $crate::Binding<String>,
            ) {
                $View::combo(self, parent, id, items, selected)
            }
            fn slider_value(&self, id: &str) -> Option<f64> {
                $View::slider_value(self, id)
            }
        }

        impl $crate::MediaView for $View {
            fn image(
                &self,
                parent: &$Parent,
                id: &str,
                source: $crate::Binding<Option<$crate::ImageFrame>>,
            ) {
                $View::image(self, parent, id, source)
            }
            fn image_with_pointer(
                &self,
                parent: &$Parent,
                id: &str,
                source: $crate::Binding<Option<$crate::ImageFrame>>,
                pointer: $crate::Binding<Option<$crate::PointerPosition>>,
            ) {
                $View::image_with_pointer(self, parent, id, source, pointer)
            }
            fn image_update_count(&self) -> usize {
                $View::image_update_count(self)
            }
            fn meter(&self, parent: &$Parent, id: &str, value: $crate::Binding<f64>) {
                $View::meter(self, parent, id, value)
            }
        }

        impl $crate::TabView for $View {
            fn tab(
                &self,
                parent: &$Parent,
                id: &str,
                selected: $crate::Binding<String>,
            ) -> $Parent {
                $View::tab(self, parent, id, selected)
            }
            fn navigation(
                &self,
                parent: &$Parent,
                id: &str,
                selected: $crate::Binding<String>,
            ) -> $Parent {
                $View::navigation(self, parent, id, selected)
            }
            fn tab_position(&self, id: &str, position: $crate::TabPosition) -> Result<(), String> {
                $View::tab_position(self, id, position)
            }
            fn tab_page(
                &self,
                parent: &$Parent,
                id: &str,
                title: $crate::Binding<String>,
                key: &str,
            ) -> $Parent {
                $View::tab_page(self, parent, id, title, key)
            }
            fn try_tab_page(
                &self,
                parent: &$Parent,
                id: &str,
                title: $crate::Binding<String>,
                key: &str,
            ) -> Result<$Parent, String> {
                $View::try_tab_page(self, parent, id, title, key)
            }
            fn finish_tab(&self, parent: &$Parent) -> Result<(), String> {
                $View::finish_tab(self, parent)
            }
            fn tab_items<T: Clone + 'static>(
                &self,
                parent: &$Parent,
                id: &str,
                items: $crate::Binding<Vec<$crate::Keyed<T>>>,
                selected: $crate::Binding<String>,
                title: ::std::rc::Rc<dyn Fn($crate::Store<T>) -> $crate::Binding<String>>,
                template: ::std::rc::Rc<dyn Fn(&View, &$Parent, &str, $crate::Store<T>)>,
            ) -> $Parent {
                $View::tab_items(self, parent, id, items, selected, title, template)
            }
        }

        impl $crate::ListView for $View {
            fn repeat<T: Clone + 'static>(
                &self,
                parent: &$Parent,
                id: &str,
                items: $crate::Binding<Vec<$crate::Keyed<T>>>,
                gap: f32,
                template: ::std::rc::Rc<dyn Fn(&View, &$Parent, &str, $crate::Store<T>)>,
            ) {
                $View::repeat(self, parent, id, items, gap, template)
            }
            fn items<T: Clone + 'static>(
                &self,
                parent: &$Parent,
                id: &str,
                items: $crate::Binding<Vec<$crate::Keyed<T>>>,
                viewport: $crate::Binding<$crate::Viewport>,
                template: ::std::rc::Rc<dyn Fn(&View, &$Parent, &str, $crate::Store<T>)>,
            ) {
                $View::items(self, parent, id, items, viewport, template)
            }
            fn content<T: Clone + 'static>(
                &self,
                parent: &$Parent,
                id: &str,
                item: $crate::Binding<Option<$crate::Keyed<T>>>,
                template: ::std::rc::Rc<dyn Fn(&View, &$Parent, &str, $crate::Store<T>)>,
            ) {
                $View::content(self, parent, id, item, template)
            }
            fn item_keys(&self, id: &str) -> Vec<String> {
                $View::item_keys(self, id)
            }
            fn mounted_count(&self, id: &str) -> usize {
                $View::mounted_count(self, id)
            }
            fn viewport(&self, id: &str, start: usize) {
                $View::viewport(self, id, start)
            }
            fn items_error(&self, id: &str) -> Option<String> {
                $View::items_error(self, id)
            }
        }

        impl $crate::DialogView for $View {
            fn dialog(
                &self,
                parent: &$Parent,
                id: &str,
                title: $crate::Binding<String>,
                open: $crate::Binding<bool>,
            ) -> $Parent {
                $View::dialog(self, parent, id, title, open)
            }
        }

        impl $crate::TableView for $View {
            fn table(&self, parent: &$Parent, id: &str, data: $crate::Binding<$crate::Table>) {
                $View::table(self, parent, id, data)
            }
            fn table_with_selection(
                &self,
                parent: &$Parent,
                id: &str,
                data: $crate::Binding<$crate::Table>,
                selection: $crate::Binding<Option<$crate::TableSelection>>,
            ) {
                $View::table_with_selection(self, parent, id, data, selection)
            }
        }

        impl $crate::MenuView for $View {
            fn typed_menu<A: Clone + 'static>(
                &self,
                id: &str,
                title: $crate::Binding<String>,
                items: $crate::Binding<Vec<$crate::MenuItem<A>>>,
                dispatch: ::std::rc::Rc<dyn Fn(A) -> bool>,
            ) {
                $View::typed_menu(self, id, title, items, dispatch)
            }
            fn hierarchical_menu<A: Clone + PartialEq + 'static>(
                &self,
                id: &str,
                title: $crate::Binding<String>,
                items: $crate::Binding<Vec<$crate::MenuNode<A>>>,
                dispatch: ::std::rc::Rc<dyn Fn(A) -> bool>,
            ) {
                $View::hierarchical_menu(self, id, title, items, dispatch)
            }
            fn activate_menu(&self, id: &str) -> Result<(), String> {
                $View::activate_menu(self, id)
            }
        }

        impl $crate::AutomationView for $View {
            fn edit(&self, id: &str, value: &str) -> Result<(), String> {
                $View::edit(self, id, value)
            }
            fn click(&self, id: &str) -> Result<(), String> {
                $View::click(self, id)
            }
            fn toggle(&self, id: &str, value: bool) -> Result<(), String> {
                $View::toggle(self, id, value)
            }
            fn slide(&self, id: &str, value: f64) -> Result<(), String> {
                $View::slide(self, id, value)
            }
            fn select(&self, id: &str, value: &str) -> Result<(), String> {
                $View::select(self, id, value)
            }
            fn set_open(&self, id: &str, value: bool) -> Result<(), String> {
                $View::set_open(self, id, value)
            }
            fn text(&self, id: &str) -> Option<String> {
                $View::text(self, id)
            }
        }

        impl $crate::Backend for $Backend {
            type View = $View;
            type Host = $Host;
            const NAME: &'static str = $name;
            fn run_host(
                make: impl FnOnce(::std::rc::Rc<$Host>) -> Result<(), String> + 'static,
            ) -> Result<(), String> {
                $run_host(make)
            }
            fn set_allows_automatic_window_tabbing(enabled: bool) {
                $set_tabbing(enabled)
            }
            fn set_initial_window_size(width: u32, height: u32) {
                $set_size(width, height)
            }
        }
    };
}
