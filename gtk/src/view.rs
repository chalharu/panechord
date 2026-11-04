//! Shared-contract implementation: `panechord_runtime` capability traits for
//! this backend's concrete `View`.
//!
//! The inherent `impl View` blocks keep their shape so existing call sites are
//! untouched; each trait method delegates to the inherent one. Generic code
//! (codegen output, `Runner`) bounds on the traits instead of naming this
//! concrete type (DIP/LSP), and screens only bound on the capabilities they
//! use (ISP). Backend-only extensions (`mount_native`, `bind_native`,
//! `native_window`, probes) stay inherent: the Open/Closed extension point.

use super::{GtkHost, Parent, View, WeakView};
use panechord_runtime::Binding;
use panechord_runtime::{
    AutomationView, Backend as BackendTrait, ButtonView, Choice, Command, DialogFooter, DialogView,
    ImageFrame, Keyed, Layout, LayoutView, ListView, MediaView, MenuItem, MenuNode, MenuView,
    PointerPosition, SelectView, SliderRange, Store, Style, Subscription, TabPosition, TabView,
    Table, TableSelection, TableView, TextView, ViewCore, Viewport, WeakViewHandle,
};
use std::rc::Rc;

impl WeakViewHandle for WeakView {
    type View = View;
    fn upgrade(&self) -> Option<View> {
        WeakView::upgrade(self)
    }
}

impl ViewCore for View {
    type Parent = Parent;
    type Weak = WeakView;
    fn new(title: Binding<String>) -> Self {
        View::new(title)
    }
    fn root(&self) -> Parent {
        View::root(self)
    }
    fn title(&self) -> String {
        View::title(self)
    }
    fn present(&self) {
        View::present(self)
    }
    fn close(&self) {
        View::close(self)
    }
    fn request_close(&self) {
        View::request_close(self)
    }
    fn on_close(&self, callback: Rc<dyn Fn()>) {
        View::on_close(self, callback)
    }
    fn on_close_request(&self, callback: Rc<dyn Fn()>) {
        View::on_close_request(self, callback)
    }
    fn downgrade(&self) -> WeakView {
        View::downgrade(self)
    }
    fn keep(&self, token: Subscription) {
        View::keep(self, token)
    }
    fn visible(&self, id: &str, binding: Binding<bool>) {
        View::visible(self, id, binding)
    }
    fn enabled(&self, id: &str, binding: Binding<bool>) {
        View::enabled(self, id, binding)
    }
    fn style_binding(&self, id: &str, binding: Binding<Style>) {
        View::style_binding(self, id, binding)
    }
    fn error(&self) -> Option<String> {
        View::error(self)
    }
    fn report_error(&self, error: String) {
        View::report_error(self, error)
    }
}

impl LayoutView for View {
    fn layout(&self, parent: &Parent, id: &str, layout: Layout) -> Parent {
        View::layout(self, parent, id, layout)
    }
    fn try_layout(&self, parent: &Parent, id: &str, layout: Layout) -> Result<Parent, String> {
        View::try_layout(self, parent, id, layout)
    }
    fn try_scroll(&self, parent: &Parent, id: &str, horizontal: bool) -> Result<Parent, String> {
        View::try_scroll(self, parent, id, horizontal)
    }
    fn style(&self, id: &str, style: Style) {
        View::style(self, id, style)
    }
    fn try_style(&self, id: &str, style: Style) -> Result<(), String> {
        View::try_style(self, id, style)
    }
    fn separator(&self, parent: &Parent, id: &str, vertical: bool) {
        View::separator(self, parent, id, vertical)
    }
}

// Re-exported for the trait path above without a name clash.

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
        placeholder: Binding<String>,
    ) {
        View::entry_with_placeholder(self, parent, id, text, placeholder)
    }
}

impl ButtonView for View {
    fn button(&self, parent: &Parent, id: &str, text: Binding<String>, command: Command) {
        View::button(self, parent, id, text, command)
    }
    fn dialog_footer(&self, parent: &Parent, id: &str, footer: DialogFooter) {
        View::dialog_footer(self, parent, id, footer)
    }
}

impl SelectView for View {
    fn checkbox(&self, parent: &Parent, id: &str, text: Binding<String>, checked: Binding<bool>) {
        View::checkbox(self, parent, id, text, checked)
    }
    fn radio(
        &self,
        parent: &Parent,
        id: &str,
        text: Binding<String>,
        selected: Binding<String>,
        value: &str,
    ) {
        View::radio(self, parent, id, text, selected, value)
    }
    fn slider(&self, parent: &Parent, id: &str, value: Binding<f64>, range: SliderRange) {
        View::slider(self, parent, id, value, range)
    }
    fn combo(
        &self,
        parent: &Parent,
        id: &str,
        items: Binding<Vec<Choice>>,
        selected: Binding<String>,
    ) {
        View::combo(self, parent, id, items, selected)
    }
    fn slider_value(&self, id: &str) -> Option<f64> {
        View::slider_value(self, id)
    }
}

impl MediaView for View {
    fn image(&self, parent: &Parent, id: &str, source: Binding<Option<ImageFrame>>) {
        View::image(self, parent, id, source)
    }
    fn image_with_pointer(
        &self,
        parent: &Parent,
        id: &str,
        source: Binding<Option<ImageFrame>>,
        pointer: Binding<Option<PointerPosition>>,
    ) {
        View::image_with_pointer(self, parent, id, source, pointer)
    }
    fn image_update_count(&self) -> usize {
        View::image_update_count(self)
    }
    fn meter(&self, parent: &Parent, id: &str, value: Binding<f64>) {
        View::meter(self, parent, id, value)
    }
}

impl TabView for View {
    fn tab(&self, parent: &Parent, id: &str, selected: Binding<String>) -> Parent {
        View::tab(self, parent, id, selected)
    }
    fn navigation(&self, parent: &Parent, id: &str, selected: Binding<String>) -> Parent {
        View::navigation(self, parent, id, selected)
    }
    fn tab_position(&self, id: &str, position: TabPosition) -> Result<(), String> {
        View::tab_position(self, id, position)
    }
    fn tab_page(&self, parent: &Parent, id: &str, title: Binding<String>, key: &str) -> Parent {
        View::tab_page(self, parent, id, title, key)
    }
    fn try_tab_page(
        &self,
        parent: &Parent,
        id: &str,
        title: Binding<String>,
        key: &str,
    ) -> Result<Parent, String> {
        View::try_tab_page(self, parent, id, title, key)
    }
    fn finish_tab(&self, parent: &Parent) -> Result<(), String> {
        View::finish_tab(self, parent)
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
        gap: f32,
        template: Rc<dyn Fn(&View, &Parent, &str, Store<T>)>,
    ) {
        View::repeat(self, parent, id, items, gap, template)
    }
    fn items<T: Clone + 'static>(
        &self,
        parent: &Parent,
        id: &str,
        items: Binding<Vec<Keyed<T>>>,
        viewport: Binding<Viewport>,
        template: Rc<dyn Fn(&View, &Parent, &str, Store<T>)>,
    ) {
        View::items(self, parent, id, items, viewport, template)
    }
    fn content<T: Clone + 'static>(
        &self,
        parent: &Parent,
        id: &str,
        item: Binding<Option<Keyed<T>>>,
        template: Rc<dyn Fn(&View, &Parent, &str, Store<T>)>,
    ) {
        View::content(self, parent, id, item, template)
    }
    fn item_keys(&self, id: &str) -> Vec<String> {
        View::item_keys(self, id)
    }
    fn mounted_count(&self, id: &str) -> usize {
        View::mounted_count(self, id)
    }
    fn viewport(&self, id: &str, start: usize) {
        View::viewport(self, id, start)
    }
    fn items_error(&self, id: &str) -> Option<String> {
        View::items_error(self, id)
    }
}

impl DialogView for View {
    fn dialog(
        &self,
        parent: &Parent,
        id: &str,
        title: Binding<String>,
        open: Binding<bool>,
    ) -> Parent {
        View::dialog(self, parent, id, title, open)
    }
}

impl TableView for View {
    fn table(&self, parent: &Parent, id: &str, data: Binding<Table>) {
        View::table(self, parent, id, data)
    }
    fn table_with_selection(
        &self,
        parent: &Parent,
        id: &str,
        data: Binding<Table>,
        selection: Binding<Option<TableSelection>>,
    ) {
        View::table_with_selection(self, parent, id, data, selection)
    }
}

impl MenuView for View {
    fn typed_menu<A: Clone + 'static>(
        &self,
        id: &str,
        title: Binding<String>,
        items: Binding<Vec<MenuItem<A>>>,
        dispatch: Rc<dyn Fn(A) -> bool>,
    ) {
        View::typed_menu(self, id, title, items, dispatch)
    }
    fn hierarchical_menu<A: Clone + PartialEq + 'static>(
        &self,
        id: &str,
        title: Binding<String>,
        items: Binding<Vec<MenuNode<A>>>,
        dispatch: Rc<dyn Fn(A) -> bool>,
    ) {
        View::hierarchical_menu(self, id, title, items, dispatch)
    }
    fn activate_menu(&self, id: &str) -> Result<(), String> {
        View::activate_menu(self, id)
    }
}

impl AutomationView for View {
    fn edit(&self, id: &str, value: &str) -> Result<(), String> {
        View::edit(self, id, value)
    }
    fn click(&self, id: &str) -> Result<(), String> {
        View::click(self, id)
    }
    fn toggle(&self, id: &str, value: bool) -> Result<(), String> {
        View::toggle(self, id, value)
    }
    fn slide(&self, id: &str, value: f64) -> Result<(), String> {
        View::slide(self, id, value)
    }
    fn select(&self, id: &str, value: &str) -> Result<(), String> {
        View::select(self, id, value)
    }
    fn set_open(&self, id: &str, value: bool) -> Result<(), String> {
        View::set_open(self, id, value)
    }
    fn text(&self, id: &str) -> Option<String> {
        View::text(self, id)
    }
}

/// This backend's [`BackendTrait`] implementation. The facade drives startup
/// through this type instead of naming `panechord_gtk` free functions per
/// call site, so a new backend adds an `impl` rather than editing dispatch.
pub struct Backend;

impl BackendTrait for Backend {
    type View = View;
    type Host = GtkHost;
    const NAME: &'static str = "gtk";
    fn run_host(
        make: impl FnOnce(Rc<GtkHost>) -> Result<(), String> + 'static,
    ) -> Result<(), String> {
        super::run_host(make)
    }
    fn set_allows_automatic_window_tabbing(enabled: bool) {
        super::set_allows_automatic_window_tabbing(enabled)
    }
    fn set_initial_window_size(width: u32, height: u32) {
        super::set_initial_window_size(width, height)
    }
}
