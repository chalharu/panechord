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
//! `runner_config` / `run_host` / option-setter shape per backend, so adding a
//! backend implements the trait instead of editing the facade dispatch.
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

/// Tab sets, virtualized lists, content projection, tables, and dialogs.
pub trait ContainerView: ViewCore {
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
    fn dialog(
        &self,
        parent: &Self::Parent,
        id: &str,
        title: Binding<String>,
        open: Binding<bool>,
    ) -> Self::Parent;
    fn table(&self, parent: &Self::Parent, id: &str, data: Binding<Table>);
    fn table_with_selection(
        &self,
        parent: &Self::Parent,
        id: &str,
        data: Binding<Table>,
        selection: Binding<Option<TableSelection>>,
    );
    fn item_keys(&self, id: &str) -> Vec<String>;
    fn mounted_count(&self, id: &str) -> usize;
    fn viewport(&self, id: &str, start: usize);
    fn items_error(&self, id: &str) -> Option<String>;
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
    + ContainerView
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
        + ContainerView
        + MenuView
        + AutomationView
{
}
