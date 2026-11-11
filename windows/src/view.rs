//! Retained, UI-local runtime view over the published Reactor 0.100.0 declarations.
//! Legacy Reactor render() is a pure diagnostic snapshot, not the product renderer.
//! The default owned SDK `owned_render` projects native Visibility and TextBox IsReadOnly directly.
//! Flex is not approximated by a fixed-cell wrap grid. Legacy snapshots use
//! zero-size/opacity wrappers; every event checks the complete ancestor chain.
//! Return/IME is not claimed. Legacy readonly snapshots disable entries.
//! Code font-family and inherited/TextBox text styles remain rejected before
//! changing retained state, including on the native renderer path.
use panechord_runtime::{
    Alignment, AutomationView, Binding, ButtonView, Choice, Command, DialogFooter, DialogView,
    Edges, ImageFrame, Keyed, Layout, LayoutView, Lifecycle, ListView, MediaView, MenuItem,
    MenuNode, MenuRole, MenuView, PointerPosition, SelectView, SliderRange, Store, Style,
    Subscription, TabPosition, TabView, Table, TableSelection, TableView, Template, TextOverflow,
    TextRole, TextStyle, TextView, ViewCore, Viewport, WeakViewHandle, panic_message,
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::{Rc, Weak},
};
use windows_reactor::{
    Border, Button, Callback, Canvas, CanvasChildExt, CheckBox, ChildrenControl, Color, ComboBox,
    ContentControl, ContentDialog, ContentDialogResult, EncodedImage, FontWeight, Grid,
    GridChildExt, GridLength, HorizontalAlignment, Image, KeyedView, LayoutControl, Menu, MenuBar,
    MenuBarItem, MenuBarSlot, MenuExt, Orientation, PointerEventInfo, ProgressBar, RadioButton,
    ScrollBarVisibility, ScrollViewer, SlotsControl, StackPanel, TextBlock, TextBox, TextTrimming,
    TextWrapping, Thickness, VerticalAlignment, View as ReactorView,
};

// Shared macro-generated Window appearance targets this reserved ID.
const ROOT: &str = "__root";
const MAX_NODES: usize = 8192;
const MAX_DEPTH: usize = 64;
const MAX_MOUNTS: usize = 256;
#[path = "owned_flex.rs"]
mod owned_flex;
#[path = "owned_render.rs"]
pub(crate) mod owned_render;
type ViewportCallback = Rc<dyn Fn(usize)>;
type PendingCollection<T> = (Vec<Keyed<T>>, Option<Viewport>);
#[derive(Clone)]
pub struct View(pub(super) Rc<Inner>, Option<Rc<Scope>>);
#[derive(Clone)]
pub struct WeakView(Weak<Inner>, Option<Weak<Scope>>);
#[derive(Clone)]
pub struct Parent {
    owner: Weak<Inner>,
    generation: u64,
    node: Weak<Node>,
}
impl std::fmt::Debug for Parent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parent")
            .field("generation", &self.generation)
            .field("id", &self.node.upgrade().map(|node| node.id.clone()))
            .finish()
    }
}
pub(super) struct Inner {
    lifecycle: Lifecycle,
    generation: Cell<u64>,
    nodes: RefCell<BTreeMap<String, Rc<Node>>>,
    tokens: RefCell<Vec<Subscription>>,
    title: RefCell<String>,
    error: RefCell<Option<String>>,
    dirty: Cell<bool>,
    notifying: Cell<bool>,
    pending_notification: Cell<bool>,
    invalidate: RefCell<Option<Rc<dyn Fn()>>>,
    native_close: RefCell<Option<Rc<dyn Fn()>>>,
    native_close_pending: Cell<bool>,
    native_edit: RefCell<Option<NativeEdit>>,
    present: RefCell<Option<Rc<dyn Fn()>>>,
    host_request: RefCell<Option<Rc<dyn Fn()>>>,
    image_updates: Cell<usize>,
    pub(super) host: RefCell<Option<Weak<super::host::HostInner>>>,
}
struct Scope {
    prefix: String,
    alive: Cell<bool>,
    tokens: RefCell<Vec<Subscription>>,
}
impl Scope {
    fn retire(&self) {
        self.alive.set(false);
        let tokens = self.tokens.take();
        drop(tokens);
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        self.retire();
    }
}
struct Node {
    id: String,
    parent: Option<String>,
    depth: usize,
    kind: Kind,
    alive: Cell<bool>,
    children: RefCell<Vec<String>>,
    text: RefCell<String>,
    placeholder: RefCell<String>,
    visible: Cell<bool>,
    enabled: Cell<bool>,
    command_enabled: Cell<bool>,
    style: Cell<Style>,
    tokens: RefCell<Vec<Subscription>>,
    applicability: RefCell<BTreeMap<&'static str, Subscription>>,
    property_versions: RefCell<BTreeMap<&'static str, u64>>,
    editor_revision: Cell<u64>,
    snapshot_revision: Cell<u64>,
    cleanup: RefCell<Vec<Rc<dyn Fn()>>>,
    text_changed: RefCell<Option<Callback<String>>>,
    clicked: RefCell<Option<Callback<()>>>,
    checked_changed: RefCell<Option<Callback<bool>>>,
    value_changed: RefCell<Option<Callback<f64>>>,
    selection_changed: RefCell<Option<Callback<Option<usize>>>>,
    pointer_moved: RefCell<Option<Callback<PointerEventInfo>>>,
    pointer_exited: RefCell<Option<Callback<PointerEventInfo>>>,
    dialog_closed: RefCell<Option<Callback<ContentDialogResult>>>,
    dynamic: RefCell<Option<Rc<dyn Any>>>,
    viewport_changed: RefCell<Option<ViewportCallback>>,
    viewport_info: Cell<Option<Viewport>>,
    previous_page: RefCell<Option<Callback<()>>>,
    next_page: RefCell<Option<Callback<()>>>,
    item_keys: RefCell<Vec<String>>,
    item_error: RefCell<Option<String>>,
}
enum Kind {
    Panel(Layout),
    Scroll(bool),
    Label,
    Entry(Binding<String>),
    Button(Command),
    Separator(bool),
    Checkbox(Binding<bool>, Cell<bool>),
    Radio(Binding<String>, String, RefCell<String>),
    Slider(Binding<f64>, SliderRange, Cell<f64>),
    Combo(Binding<String>, RefCell<Vec<Choice>>, RefCell<String>),
    Image(
        RefCell<Option<ImageFrame>>,
        RefCell<Option<EncodedImage>>,
        Option<Binding<Option<PointerPosition>>>,
    ),
    Meter(Cell<f64>),
    Tabs(TabState),
    Page(String),
    Dialog(Binding<bool>, Cell<bool>),
    Table(TableState),
    TableCell(TableSelection),
    Menu(MenuState),
}
struct TabState {
    selected: Binding<String>,
    value: RefCell<String>,
    position: Cell<TabPosition>,
    finished: Cell<bool>,
}
struct TableState {
    data: RefCell<Option<Table>>,
    selection: Option<Binding<Option<TableSelection>>>,
    selected: RefCell<Option<TableSelection>>,
    cells: RefCell<BTreeMap<String, Rc<Node>>>,
}
struct MenuState {
    native: RefCell<Vec<windows_reactor::MenuItem>>,
    native_ids: RefCell<std::collections::HashMap<windows_reactor::Key, String>>,
    actions: RefCell<BTreeMap<String, Rc<dyn Fn() -> bool>>>,
    metadata: RefCell<BTreeMap<String, MenuMetadata>>,
    callback: RefCell<Option<Callback<String>>>,
}
type NativeEdit = Rc<dyn Fn(MenuRole) -> Result<(), String>>;
#[derive(Clone)]
struct MenuMetadata {
    role: MenuRole,
    chord: Option<panechord_runtime::Shortcut>,
    enabled: bool,
}
impl Node {
    fn new(id: String, parent: Option<String>, depth: usize, kind: Kind) -> Rc<Self> {
        Rc::new(Self {
            id,
            parent,
            depth,
            kind,
            alive: Cell::new(true),
            children: RefCell::default(),
            text: RefCell::default(),
            placeholder: RefCell::default(),
            visible: Cell::new(true),
            enabled: Cell::new(true),
            command_enabled: Cell::new(true),
            style: Cell::default(),
            tokens: RefCell::default(),
            applicability: RefCell::default(),
            property_versions: RefCell::default(),
            editor_revision: Cell::new(0),
            snapshot_revision: Cell::new(0),
            cleanup: RefCell::default(),
            text_changed: RefCell::default(),
            clicked: RefCell::default(),
            checked_changed: RefCell::default(),
            value_changed: RefCell::default(),
            selection_changed: RefCell::default(),
            pointer_moved: RefCell::default(),
            pointer_exited: RefCell::default(),
            dialog_closed: RefCell::default(),
            dynamic: RefCell::default(),
            viewport_changed: RefCell::default(),
            viewport_info: Cell::new(None),
            previous_page: RefCell::default(),
            next_page: RefCell::default(),
            item_keys: RefCell::default(),
            item_error: RefCell::default(),
        })
    }
    fn container(&self) -> bool {
        matches!(
            self.kind,
            Kind::Panel(_)
                | Kind::Scroll(_)
                | Kind::Tabs(_)
                | Kind::Page(_)
                | Kind::Dialog(..)
                | Kind::Table(_)
        )
    }
    fn layout(&self) -> Layout {
        match self.kind {
            Kind::Panel(layout) => layout,
            _ => Layout::default(),
        }
    }
    fn retire(&self) {
        self.alive.set(false);
        let tokens = self.tokens.take();
        let applicability = self.applicability.take();
        let dynamic = self.dynamic.take();
        let viewport = self.viewport_changed.take();
        let cleanup = self.cleanup.take();
        for cleanup in cleanup {
            cleanup();
        }
        drop(tokens);
        drop(applicability);
        drop(dynamic);
        drop(viewport);
    }
}
impl Inner {
    fn report(&self, error: String) {
        if self.error.borrow().is_none() {
            self.error.replace(Some(error));
        }
    }
    fn attempt(&self, action: impl FnOnce()) {
        if let Err(panic) = catch_unwind(AssertUnwindSafe(action)) {
            self.report(format!("callback panicked: {}", panic_message(&*panic)));
        }
    }
    fn notify(&self) {
        if self.lifecycle.is_closed() {
            return;
        }
        self.pending_notification.set(true);
        if self.notifying.replace(true) {
            return;
        }
        for _ in 0..64 {
            self.pending_notification.set(false);
            let callback = self.invalidate.borrow().clone();
            if let Some(callback) = callback {
                self.attempt(|| callback());
            }
            if self.lifecycle.is_closed() || !self.pending_notification.get() {
                break;
            }
        }
        if self.pending_notification.replace(false) {
            self.report("view notification feedback limit".into());
        }
        self.notifying.set(false);
    }
    fn changed(&self) {
        if !self.lifecycle.is_closed() {
            self.dirty.set(true);
            self.notify();
        }
    }
    fn shutdown(&self) {
        if !self.lifecycle.begin_shutdown() {
            return;
        }
        self.generation.set(self.generation.get().wrapping_add(1));
        let nodes = self.nodes.take();
        let tokens = self.tokens.take();
        let invalidate = self.invalidate.take();
        let present = self.present.take();
        let request = self.host_request.take();
        let native = self.native_close.take();
        self.native_edit.take();
        self.native_close_pending.set(true);
        self.dirty.set(false);
        for node in nodes.into_values() {
            self.attempt(|| node.retire());
        }
        for token in tokens {
            self.attempt(|| drop(token));
        }
        self.attempt(|| drop(invalidate));
        self.attempt(|| drop(present));
        self.attempt(|| drop(request));
        if let Some(native) = native {
            self.native_close_pending.set(false);
            self.attempt(|| native());
        }
        self.lifecycle
            .finish_shutdown(|error| self.report(format!("close: {error}")));
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.shutdown();
    }
}
impl WeakViewHandle for WeakView {
    type View = View;
    fn upgrade(&self) -> Option<View> {
        let inner = self
            .0
            .upgrade()
            .filter(|inner| !inner.lifecycle.is_closed())?;
        let scope = match &self.1 {
            Some(scope) => Some(scope.upgrade().filter(|scope| scope.alive.get())?),
            None => None,
        };
        Some(View(inner, scope))
    }
}
impl WeakView {
    pub fn upgrade(&self) -> Option<View> {
        WeakViewHandle::upgrade(self)
    }
}
impl View {
    pub fn new(title: Binding<String>) -> Self {
        <Self as ViewCore>::new(title)
    }
    pub fn is_closed(&self) -> bool {
        self.0.lifecycle.is_closed() || self.1.as_ref().is_some_and(|scope| !scope.alive.get())
    }
    pub fn same_view(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    pub fn is_dirty(&self) -> bool {
        self.0.dirty.get()
    }
    pub fn set_invalidate(&self, callback: Rc<dyn Fn()>) {
        if self.is_closed() {
            return;
        }
        let old = self.0.invalidate.replace(Some(callback));
        drop(old);
        self.0.dirty.set(true);
        self.0.notify();
    }
    pub fn clear_invalidate(&self) {
        let old = self.0.invalidate.take();
        drop(old);
    }
    pub fn set_present(&self, callback: Rc<dyn Fn()>) {
        if !self.is_closed() {
            let old = self.0.present.replace(Some(callback));
            drop(old);
        }
    }
    pub fn set_native_close(&self, callback: Rc<dyn Fn()>) {
        if self.is_closed() {
            if self.0.native_close_pending.replace(false) {
                self.0.attempt(|| callback());
            }
        } else {
            let old = self.0.native_close.replace(Some(callback));
            drop(old);
        }
    }
    pub fn native_close_request(&self) {
        self.request_close();
    }
    pub fn native_closed(&self) {
        let old = self.0.native_close.take();
        drop(old);
        self.close();
        self.0.native_close_pending.set(false);
    }
    pub(super) fn set_host_request(&self, callback: Rc<dyn Fn()>) {
        let old = self.0.host_request.replace(Some(callback));
        drop(old);
    }
    fn resolve(&self, id: &str) -> String {
        match &self.1 {
            Some(scope) if id != scope.prefix && !id.starts_with(&format!("{}/", scope.prefix)) => {
                format!("{}/{id}", scope.prefix)
            }
            _ => id.into(),
        }
    }
    fn live(&self, node: &Rc<Node>) -> bool {
        !self.is_closed()
            && node.alive.get()
            && self
                .0
                .nodes
                .borrow()
                .get(&node.id)
                .is_some_and(|current| Rc::ptr_eq(current, node))
    }
    fn find(&self, id: &str) -> Result<Rc<Node>, String> {
        if self.is_closed() {
            return Err("view or template scope is closed".into());
        }
        self.0
            .nodes
            .borrow()
            .get(&self.resolve(id))
            .cloned()
            .ok_or_else(|| format!("unknown node: {id}"))
    }
    fn parent_for(&self, node: &Rc<Node>) -> Parent {
        Parent {
            owner: Rc::downgrade(&self.0),
            generation: self.0.generation.get(),
            node: Rc::downgrade(node),
        }
    }
    fn invalid_parent(&self) -> Parent {
        Parent {
            owner: Weak::new(),
            generation: 0,
            node: Weak::new(),
        }
    }
    fn validate_parent(&self, parent: &Parent) -> Result<Rc<Node>, String> {
        if parent.generation != self.0.generation.get()
            || !parent
                .owner
                .upgrade()
                .is_some_and(|owner| Rc::ptr_eq(&owner, &self.0))
        {
            return Err("foreign, invalid or closed parent".into());
        }
        let node = parent
            .node
            .upgrade()
            .filter(|node| self.live(node))
            .ok_or("stale parent")?;
        if !node.container() {
            return Err("parent is not a container".into());
        }
        Ok(node)
    }
    fn insert(&self, parent: &Parent, id: &str, kind: Kind) -> Result<Rc<Node>, String> {
        let parent = self.validate_parent(parent)?;
        let id = self.resolve(id);
        if id.is_empty() || id == ROOT || id.len() > 2048 || id.contains('\0') {
            return Err("invalid or reserved node id".into());
        }
        if parent.depth >= MAX_DEPTH {
            return Err("tree depth limit".into());
        }
        {
            let nodes = self.0.nodes.borrow();
            if nodes.contains_key(&id) {
                return Err(format!("duplicate node: {id}"));
            }
            if nodes.len() >= MAX_NODES {
                return Err("node limit".into());
            }
        }
        let node = Node::new(id.clone(), Some(parent.id.clone()), parent.depth + 1, kind);
        self.0.nodes.borrow_mut().insert(id.clone(), node.clone());
        parent.children.borrow_mut().push(id);
        Ok(node)
    }
    fn remove_tree(&self, id: &str) {
        let node = self.0.nodes.borrow_mut().remove(id);
        if let Some(node) = node {
            node.alive.set(false);
            if let Some(parent) = &node.parent
                && let Some(parent) = self.0.nodes.borrow().get(parent)
            {
                parent.children.borrow_mut().retain(|child| child != id);
            }
            let children = node.children.take();
            for child in children {
                self.remove_tree(&child);
            }
            self.0.attempt(|| node.retire());
        }
    }
    fn bind<T: Clone + 'static>(
        &self,
        node: &Rc<Node>,
        binding: Binding<T>,
        apply: impl Fn(&View, &Rc<Node>, T) -> Result<(), String> + 'static,
    ) {
        let owner = self.downgrade();
        let target = Rc::downgrade(node);
        let token = binding.subscribe(Rc::new(move |value| {
            if let (Some(view), Some(node)) = (owner.upgrade(), target.upgrade())
                && view.live(&node)
            {
                view.0.attempt(|| {
                    if let Err(error) = apply(&view, &node, value) {
                        view.report_error(error);
                    }
                });
                view.0.changed();
            }
        }));
        if self.live(node) {
            node.tokens.borrow_mut().push(token);
        } else {
            drop(token);
        }
    }
    fn event<T: 'static>(
        &self,
        node: &Rc<Node>,
        action: impl Fn(&View, &Rc<Node>, T) -> Result<(), String> + 'static,
    ) -> Callback<T> {
        let owner = self.downgrade();
        let target = Rc::downgrade(node);
        Callback::new(move |value| {
            if let (Some(view), Some(node)) = (owner.upgrade(), target.upgrade())
                && view.live(&node)
            {
                view.0.attempt(|| {
                    let _ = action(&view, &node, value);
                });
            }
        })
    }
    fn actionable(&self, node: &Node) -> bool {
        let mut current = Some(node.id.clone());
        while let Some(id) = current {
            let node = self.0.nodes.borrow().get(&id).cloned();
            let Some(node) = node else {
                return false;
            };
            if !node.alive.get() || !node.visible.get() || !node.enabled.get() {
                return false;
            }
            if let Kind::Dialog(_, open) = &node.kind
                && !open.get()
            {
                return false;
            }
            if let Kind::Page(key) = &node.kind
                && let Some(parent) = &node.parent
            {
                let parent = self.0.nodes.borrow().get(parent).cloned();
                if let Some(parent) = parent
                    && let Kind::Tabs(tabs) = &parent.kind
                    && *tabs.value.borrow() != *key
                {
                    return false;
                }
            }
            current = node.parent.clone();
        }
        !self.is_closed()
    }
    fn write<T: Clone + 'static>(
        &self,
        node: &Rc<Node>,
        binding: &Binding<T>,
        value: T,
    ) -> Result<(), String> {
        let result = if !self.actionable(node) {
            Err("control or ancestor is hidden/disabled".into())
        } else {
            binding.try_set(value).map_err(|error| error.to_string())
        };
        if result.is_err() {
            self.recover(node);
        }
        result
    }
    fn recover(&self, node: &Node) {
        node.editor_revision
            .set(node.editor_revision.get().wrapping_add(1));
        self.0.changed();
    }
    pub fn native_text_changed(&self, id: &str, value: &str) -> Result<(), String> {
        let node = self.find(id)?;
        if !matches!(node.kind, Kind::Entry(_)) {
            return Err("target is not an entry".into());
        }
        if *node.text.borrow() == value {
            return Ok(());
        }
        self.edit(id, value)
    }
    pub fn text_changed_callback(&self, id: &str) -> Option<Callback<String>> {
        self.find(id)
            .ok()
            .and_then(|node| node.text_changed.borrow().clone())
    }
    pub fn clicked_callback(&self, id: &str) -> Option<Callback<()>> {
        self.find(id)
            .ok()
            .and_then(|node| node.clicked.borrow().clone())
    }
    /// Published native menus deliver display labels, not opaque keys.
    pub fn menu_callback(&self, id: &str) -> Option<Callback<String>> {
        self.find(id).ok().and_then(|node| {
            if let Kind::Menu(menu) = &node.kind {
                menu.callback.borrow().clone()
            } else {
                None
            }
        })
    }
    pub fn checked_changed_callback(&self, id: &str) -> Option<Callback<bool>> {
        self.find(id)
            .ok()
            .and_then(|node| node.checked_changed.borrow().clone())
    }
    pub fn value_changed_callback(&self, id: &str) -> Option<Callback<f64>> {
        self.find(id)
            .ok()
            .and_then(|node| node.value_changed.borrow().clone())
    }
    pub fn selection_changed_callback(&self, id: &str) -> Option<Callback<Option<usize>>> {
        self.find(id)
            .ok()
            .and_then(|node| node.selection_changed.borrow().clone())
    }
    pub fn node_count(&self) -> usize {
        self.0.nodes.borrow().len().saturating_sub(1)
    }
    pub fn retained_style(&self, id: &str) -> Option<Style> {
        self.find(id).ok().map(|node| node.style.get())
    }
    pub fn checked(&self, id: &str) -> Option<bool> {
        self.find(id).ok().and_then(|node| match &node.kind {
            Kind::Checkbox(_, value) => Some(value.get()),
            Kind::Radio(_, key, value) => Some(*value.borrow() == *key),
            _ => None,
        })
    }
    pub fn selected(&self, id: &str) -> Option<String> {
        self.find(id).ok().and_then(|node| match &node.kind {
            Kind::Combo(_, _, value) | Kind::Radio(_, _, value) => Some(value.borrow().clone()),
            Kind::Tabs(tabs) => Some(tabs.value.borrow().clone()),
            _ => None,
        })
    }
    pub fn meter_value(&self, id: &str) -> Option<f64> {
        self.find(id).ok().and_then(|node| {
            if let Kind::Meter(value) = &node.kind {
                Some(value.get())
            } else {
                None
            }
        })
    }
    pub fn image_bytes(&self, id: &str) -> Option<Vec<u8>> {
        self.find(id).ok().and_then(|node| {
            if let Kind::Image(_, bytes, _) = &node.kind {
                bytes
                    .borrow()
                    .as_ref()
                    .map(|bytes| bytes.as_bytes().to_vec())
            } else {
                None
            }
        })
    }
    fn mount_text(
        &self,
        parent: &Parent,
        id: &str,
        text: Binding<String>,
        entry: bool,
        placeholder: Option<Binding<String>>,
    ) {
        let result = self.insert(
            parent,
            id,
            if entry {
                Kind::Entry(text.clone())
            } else {
                Kind::Label
            },
        );
        match result {
            Ok(node) => {
                if entry {
                    node.text_changed.replace(Some(
                        self.event(&node, |view, node, value: String| {
                            view.native_text_changed(&node.id, &value)
                        }),
                    ));
                }
                self.bind(&node, text, |_, node, value| {
                    node.text.replace(value);
                    Ok(())
                });
                if let Some(placeholder) = placeholder {
                    self.bind(&node, placeholder, |_, node, value| {
                        node.placeholder.replace(value);
                        Ok(())
                    });
                }
                self.0.changed();
            }
            Err(error) => self.report_error(error),
        }
    }
    pub fn render(&self) -> ReactorView {
        if self.is_closed() {
            return Grid::new().into();
        }
        let result = self.render_node(ROOT, true, true);
        self.0.dirty.set(false);
        result
    }
    fn stack_children(&self, node: &Node, visible: bool, enabled: bool) -> ReactorView {
        let children = node.children.borrow().clone();
        let (horizontal, gap) = match node.kind {
            Kind::Panel(Layout::Stack { horizontal, gap }) => (horizontal, gap),
            _ => (false, 0.0),
        };
        StackPanel::new()
            .orientation(if horizontal {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            })
            .spacing(f64::from(gap))
            .keyed_children(
                children
                    .iter()
                    .map(|id| KeyedView::new(id.clone(), self.render_node(id, visible, enabled))),
            )
    }
    fn render_node(
        &self,
        id: &str,
        ancestors_visible: bool,
        ancestors_enabled: bool,
    ) -> ReactorView {
        let node = self
            .0
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .expect("retained node");
        let mut visible = ancestors_visible && node.visible.get();
        if let Kind::Page(_) = &node.kind {
            visible &= self.actionable_page(&node);
        }
        let enabled = ancestors_enabled && node.enabled.get() && visible;
        let children = node.children.borrow().clone();
        let content: ReactorView = match &node.kind {
            Kind::Panel(Layout::Stack { .. }) => self.stack_children(&node, visible, enabled),
            Kind::Panel(Layout::Grid { columns, gap }) => {
                let placements: Vec<_> = children
                    .iter()
                    .enumerate()
                    .map(|(index, id)| {
                        let child = self.0.nodes.borrow().get(id).cloned().expect("grid child");
                        let placement = child.style.get().placement;
                        let (row, column) = placement.grid_position(index, *columns);
                        (row, column, placement.row_span, placement.column_span)
                    })
                    .collect();
                let rows = placements
                    .iter()
                    .map(|(row, _, span, _)| row + span)
                    .max()
                    .unwrap_or(1);
                Grid::new()
                    .columns(vec![GridLength::STAR; *columns as usize])
                    .rows(vec![GridLength::Auto; rows as usize])
                    .row_spacing(f64::from(*gap))
                    .column_spacing(f64::from(*gap))
                    .keyed_children(children.iter().zip(placements).map(
                        |(id, (row, column, rs, cs))| {
                            KeyedView::new(
                                id.clone(),
                                Grid::new()
                                    .grid_row(row)
                                    .grid_column(column)
                                    .grid_row_span(rs)
                                    .grid_column_span(cs)
                                    .keyed_children([KeyedView::new(
                                        "content",
                                        self.render_node(id, visible, enabled),
                                    )]),
                            )
                        },
                    ))
            }
            Kind::Panel(Layout::Absolute) => {
                Canvas::new().keyed_children(children.iter().map(|id| {
                    let child = self
                        .0
                        .nodes
                        .borrow()
                        .get(id)
                        .cloned()
                        .expect("canvas child");
                    let p = child.style.get().placement;
                    KeyedView::new(
                        id.clone(),
                        Grid::new()
                            .canvas_left(f64::from(p.x))
                            .canvas_top(f64::from(p.y))
                            .keyed_children([KeyedView::new(
                                "content",
                                self.render_node(id, visible, enabled),
                            )]),
                    )
                }))
            }
            // Diagnostic snapshot only: Reactor DSL cannot express custom panels.
            // The product renderer uses owned_flex's native Measure/Arrange.
            Kind::Panel(Layout::Flex { .. }) => self.stack_children(&node, visible, enabled),
            Kind::Scroll(horizontal) => ScrollViewer::new()
                .horizontal_scroll_bar_visibility(if *horizontal {
                    ScrollBarVisibility::Auto
                } else {
                    ScrollBarVisibility::Disabled
                })
                .vertical_scroll_bar_visibility(if *horizontal {
                    ScrollBarVisibility::Disabled
                } else {
                    ScrollBarVisibility::Auto
                })
                .content(self.render_scroll_content(&node, visible, enabled)),
            Kind::Page(_) => self.stack_children(&node, visible, enabled),
            Kind::Label => styled_text(&node).into(),
            Kind::Entry(binding) => TextBox::new()
                .text(node.text.borrow().clone())
                .placeholder_text(node.placeholder.borrow().clone())
                .accepts_return(false)
                .is_enabled(enabled && binding.is_writable())
                .on_text_changed(
                    node.text_changed
                        .borrow()
                        .as_ref()
                        .expect("entry callback")
                        .clone(),
                )
                .into(),
            Kind::Button(_) => Button::new()
                .is_enabled(enabled && node.command_enabled.get())
                .on_click(
                    node.clicked
                        .borrow()
                        .as_ref()
                        .expect("button callback")
                        .clone(),
                )
                .content(styled_text(&node)),
            Kind::Checkbox(binding, checked) => CheckBox::new()
                .is_checked(checked.get())
                .is_enabled(enabled && binding.is_writable())
                .on_is_checked_changed(
                    node.checked_changed
                        .borrow()
                        .as_ref()
                        .expect("checkbox callback")
                        .clone(),
                )
                .content(styled_text(&node)),
            Kind::Radio(binding, value, selected) => RadioButton::new()
                .group_name(&node.id)
                .is_checked(*selected.borrow() == *value)
                .is_enabled(enabled && binding.is_writable())
                .on_checked(
                    node.checked_changed
                        .borrow()
                        .as_ref()
                        .expect("radio callback")
                        .clone(),
                )
                .content(styled_text(&node)),
            Kind::Slider(binding, range, value) => windows_reactor::Slider::new()
                .minimum(range.min)
                .maximum(range.max)
                .step_frequency(range.step)
                .value(value.get())
                .is_enabled(enabled && binding.is_writable())
                .on_value_changed(
                    node.value_changed
                        .borrow()
                        .as_ref()
                        .expect("slider callback")
                        .clone(),
                )
                .into(),
            Kind::Combo(binding, choices, selected) => {
                let choices = choices.borrow();
                ComboBox::new()
                    .is_editable(false)
                    .items_source(choices.iter().map(|choice| choice.label.clone()))
                    .selected_index(
                        choices
                            .iter()
                            .position(|choice| choice.id == *selected.borrow()),
                    )
                    .is_enabled(enabled && binding.is_writable())
                    .on_selection_changed(
                        node.selection_changed
                            .borrow()
                            .as_ref()
                            .expect("combo callback")
                            .clone(),
                    )
                    .into()
            }
            Kind::Image(_, bytes, _) => {
                let image = Image::new().stretch(windows_reactor::Stretch::Uniform);
                let image = match bytes.borrow().clone() {
                    Some(bytes) => image.source_data(bytes),
                    None => image,
                };
                let mut border = Border::new();
                if let Some(callback) = node.pointer_moved.borrow().clone() {
                    border = border.on_pointer_moved(callback);
                }
                if let Some(callback) = node.pointer_exited.borrow().clone() {
                    border = border.on_pointer_exited(callback);
                }
                border.content(image)
            }
            Kind::Meter(value) => ProgressBar::new()
                .minimum(0.0)
                .maximum(1.0)
                .value(value.get())
                .is_indeterminate(false)
                .is_enabled(enabled)
                .into(),
            Kind::Separator(vertical) => {
                let border = Border::new().background(Color::rgb(128, 128, 128));
                if *vertical {
                    border.width(1.0).into()
                } else {
                    border.height(1.0).into()
                }
            }
            Kind::Tabs(tabs) => self.render_tabs(&node, tabs, visible, enabled),
            Kind::Dialog(_, open) => ContentDialog::new()
                .title(node.text.borrow().clone())
                .is_open(open.get() && visible)
                .on_closed(
                    node.dialog_closed
                        .borrow()
                        .as_ref()
                        .expect("dialog callback")
                        .clone(),
                )
                .content(self.stack_children(&node, visible && open.get(), enabled && open.get())),
            Kind::Table(table) => self.render_table(&node, table, visible, enabled),
            Kind::TableCell(_) => Button::new()
                .is_enabled(enabled)
                .on_click(
                    node.clicked
                        .borrow()
                        .as_ref()
                        .expect("table cell callback")
                        .clone(),
                )
                .content(styled_text(&node)),
            Kind::Menu(menu) => {
                if node.text.borrow().is_empty() && menu.native.borrow().is_empty() {
                    Grid::new().into()
                } else {
                    let items = menu.native.borrow().clone();
                    let items = if enabled {
                        items
                    } else {
                        disabled_menu(&items)
                    };
                    MenuBar::new().collection_slot(
                        MenuBarSlot::Items,
                        [KeyedView::new(
                            "menu",
                            MenuBarItem::new()
                                .title(node.text.borrow().clone())
                                .menu(Menu::new(
                                    items,
                                    menu.callback
                                        .borrow()
                                        .as_ref()
                                        .expect("menu callback")
                                        .clone(),
                                )),
                        )],
                    )
                }
            }
        };
        // Reactor 0.100.0 rejects a detached ContentDialog owned by a content
        // control (Border). Retain the dialog under a children-role native owner.
        let content = if matches!(node.kind, Kind::Dialog(..)) {
            Grid::new().keyed_children([KeyedView::new("dialog", content)])
        } else {
            content
        };
        let style = node.style.get();
        let mut wrapper = Grid::new()
            .opacity(if visible { 1.0 } else { 0.0 })
            .margin(thickness(if visible {
                style.margin
            } else {
                Edges::default()
            }));
        if !visible {
            wrapper = wrapper.width(0.0).height(0.0);
        } else {
            if let Some(width) = style.placement.width {
                wrapper = wrapper.width(f64::from(width));
            }
            if let Some(height) = style.placement.height {
                wrapper = wrapper.height(f64::from(height));
            }
        }
        if let Some(alignment) = style.horizontal_alignment {
            wrapper = wrapper.horizontal_alignment(horizontal_alignment(alignment));
        }
        if let Some(alignment) = style.vertical_alignment {
            wrapper = wrapper.vertical_alignment(vertical_alignment(alignment));
        }
        wrapper.keyed_children([KeyedView::new(
            format!("content:{}", node.editor_revision.get()),
            Border::new()
                .padding(thickness(if visible {
                    style.padding
                } else {
                    Edges::default()
                }))
                .content(content),
        )])
    }
    fn actionable_page(&self, node: &Node) -> bool {
        if let Kind::Page(key) = &node.kind
            && let Some(parent) = &node.parent
        {
            let parent = self.0.nodes.borrow().get(parent).cloned();
            if let Some(parent) = parent
                && let Kind::Tabs(tabs) = &parent.kind
            {
                return *tabs.value.borrow() == *key;
            }
        }
        true
    }
}
fn thickness(edges: Edges) -> Thickness {
    Thickness::new(
        f64::from(edges.left),
        f64::from(edges.top),
        f64::from(edges.right),
        f64::from(edges.bottom),
    )
}
fn horizontal_alignment(a: Alignment) -> HorizontalAlignment {
    match a {
        Alignment::Start => HorizontalAlignment::Left,
        Alignment::Center => HorizontalAlignment::Center,
        Alignment::End => HorizontalAlignment::Right,
        Alignment::Stretch => HorizontalAlignment::Stretch,
    }
}
fn vertical_alignment(a: Alignment) -> VerticalAlignment {
    match a {
        Alignment::Start => VerticalAlignment::Top,
        Alignment::Center => VerticalAlignment::Center,
        Alignment::End => VerticalAlignment::Bottom,
        Alignment::Stretch => VerticalAlignment::Stretch,
    }
}
fn text_style(text: &str, style: TextStyle) -> TextBlock {
    let mut label = TextBlock::new()
        .text(text)
        .font_size(
            f64::from(style.scale)
                * match style.role {
                    TextRole::Heading => 28.0,
                    TextRole::Subtitle => 20.0,
                    _ => 14.0,
                },
        )
        .font_weight(
            if matches!(style.role, TextRole::Heading | TextRole::Subtitle) {
                FontWeight::SEMI_BOLD
            } else {
                FontWeight::NORMAL
            },
        );
    debug_assert_ne!(
        style.role,
        TextRole::Code,
        "Code is rejected before retaining text styles"
    );
    if let Some([r, g, b]) = style.foreground {
        label = label.foreground(Color::rgb(r, g, b));
    }
    label
}
fn styled_text(node: &Node) -> TextBlock {
    text_style(
        &node.text.borrow(),
        node.style.get().text.unwrap_or_default(),
    )
}
fn encode_image(frame: &ImageFrame) -> Result<EncodedImage, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, frame.width(), frame.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(frame.pixels())
            .map_err(|error| error.to_string())?;
        writer.finish().map_err(|error| error.to_string())?;
    }
    Ok(EncodedImage::new(bytes))
}
impl ViewCore for View {
    type Parent = Parent;
    type Weak = WeakView;
    fn new(title: Binding<String>) -> Self {
        let root = Node::new(ROOT.into(), None, 0, Kind::Panel(Layout::default()));
        let view = View(
            Rc::new(Inner {
                lifecycle: Lifecycle::default(),
                generation: Cell::new(1),
                nodes: RefCell::new(BTreeMap::from([(ROOT.into(), root)])),
                tokens: RefCell::default(),
                title: RefCell::default(),
                error: RefCell::default(),
                dirty: Cell::new(true),
                notifying: Cell::new(false),
                pending_notification: Cell::new(false),
                invalidate: RefCell::default(),
                native_close: RefCell::default(),
                native_close_pending: Cell::new(false),
                native_edit: RefCell::default(),
                present: RefCell::default(),
                host_request: RefCell::default(),
                host: RefCell::default(),
                image_updates: Cell::new(0),
            }),
            None,
        );
        super::host::track_making_view(&view);
        let weak = view.downgrade();
        let token = title.subscribe(Rc::new(move |value| {
            if let Some(view) = weak.upgrade() {
                view.0.title.replace(value);
                view.0.changed();
            }
        }));
        view.keep(token);
        view
    }
    fn root(&self) -> Parent {
        let id = self.1.as_ref().map_or(ROOT, |scope| scope.prefix.as_str());
        self.0
            .nodes
            .borrow()
            .get(id)
            .map(|node| self.parent_for(node))
            .unwrap_or_else(|| self.invalid_parent())
    }
    fn title(&self) -> String {
        self.0.title.borrow().clone()
    }
    fn present(&self) {
        if self.is_closed() {
            return;
        }
        let callback = self.0.present.borrow().clone();
        if let Some(callback) = callback {
            self.0.attempt(|| callback());
        }
        self.0.changed();
    }
    fn close(&self) {
        self.0.shutdown();
    }
    fn request_close(&self) {
        let Some(_guard) = self.0.lifecycle.enter_request() else {
            return;
        };
        let request = self
            .0
            .lifecycle
            .close_request()
            .or_else(|| self.0.host_request.borrow().clone());
        if let Some(request) = request {
            self.0.attempt(|| request());
        } else {
            self.close();
        }
    }
    fn on_close(&self, callback: Rc<dyn Fn()>) {
        self.0.attempt(|| self.0.lifecycle.on_close(callback));
    }
    fn on_close_request(&self, callback: Rc<dyn Fn()>) {
        if !self.is_closed() {
            self.0.lifecycle.set_close_request(Some(callback));
        }
    }
    fn downgrade(&self) -> WeakView {
        WeakView(Rc::downgrade(&self.0), self.1.as_ref().map(Rc::downgrade))
    }
    fn keep(&self, token: Subscription) {
        if self.is_closed() {
            self.0.attempt(|| drop(token));
        } else if let Some(scope) = &self.1 {
            scope.tokens.borrow_mut().push(token);
        } else {
            self.0.tokens.borrow_mut().push(token);
        }
    }
    fn visible(&self, id: &str, binding: Binding<bool>) {
        self.applicability(id, binding, "visible", |_, node, value| {
            node.visible.set(value);
            Ok(())
        });
    }
    fn enabled(&self, id: &str, binding: Binding<bool>) {
        self.applicability(id, binding, "enabled", |_, node, value| {
            node.enabled.set(value);
            Ok(())
        });
    }
    fn style_binding(&self, id: &str, binding: Binding<Style>) {
        self.applicability(id, binding, "style", |view, node, value| {
            view.try_style(&node.id, value)
        });
    }
    fn error(&self) -> Option<String> {
        self.0.error.borrow().clone()
    }
    fn report_error(&self, error: String) {
        self.0.report(error);
    }
}
impl View {
    fn applicability<T: Clone + 'static>(
        &self,
        id: &str,
        binding: Binding<T>,
        property: &'static str,
        apply: impl Fn(&View, &Rc<Node>, T) -> Result<(), String> + 'static,
    ) {
        let Ok(node) = self.find(id) else {
            self.report_error(format!("unknown node: {id}"));
            return;
        };
        let version = node
            .property_versions
            .borrow()
            .get(property)
            .copied()
            .unwrap_or(0)
            .wrapping_add(1);
        node.property_versions
            .borrow_mut()
            .insert(property, version);
        let old = node.applicability.borrow_mut().remove(property);
        drop(old);
        if !self.live(&node) || node.property_versions.borrow().get(property) != Some(&version) {
            return;
        }
        let owner = self.downgrade();
        let target = Rc::downgrade(&node);
        let token = binding.subscribe(Rc::new(move |value| {
            if let (Some(view), Some(node)) = (owner.upgrade(), target.upgrade())
                && view.live(&node)
            {
                let current = node.property_versions.borrow().get(property) == Some(&version);
                if current {
                    view.0.attempt(|| {
                        if let Err(error) = apply(&view, &node, value) {
                            view.report_error(error);
                        }
                    });
                    view.0.changed();
                }
            }
        }));
        if self.live(&node) && node.property_versions.borrow().get(property) == Some(&version) {
            let old = node.applicability.borrow_mut().insert(property, token);
            drop(old);
        } else {
            drop(token);
        }
    }
}
impl LayoutView for View {
    fn layout(&self, parent: &Parent, id: &str, layout: Layout) -> Parent {
        match self.try_layout(parent, id, layout) {
            Ok(parent) => parent,
            Err(error) => {
                self.report_error(error);
                self.invalid_parent()
            }
        }
    }
    fn try_layout(&self, parent: &Parent, id: &str, layout: Layout) -> Result<Parent, String> {
        layout.validate().map_err(str::to_owned)?;
        let node = self.insert(parent, id, Kind::Panel(layout))?;
        self.0.changed();
        Ok(self.parent_for(&node))
    }
    fn try_scroll(&self, parent: &Parent, id: &str, horizontal: bool) -> Result<Parent, String> {
        let node = self.insert(parent, id, Kind::Scroll(horizontal))?;
        self.0.changed();
        Ok(self.parent_for(&node))
    }
    fn style(&self, id: &str, style: Style) {
        if let Err(error) = self.try_style(id, style) {
            self.report_error(error);
        }
    }
    fn try_style(&self, id: &str, style: Style) -> Result<(), String> {
        let node = self.find(id)?;
        let parent = node
            .parent
            .as_ref()
            .and_then(|id| self.0.nodes.borrow().get(id).cloned());
        style
            .validate(
                parent
                    .as_ref()
                    .map_or(Layout::default(), |node| node.layout()),
            )
            .map_err(str::to_owned)?;
        if let Some(parent) = &parent
            && let Layout::Grid { columns, .. } = parent.layout()
            && style.placement.column + style.placement.column_span > columns as i32
        {
            return Err("Grid placement exceeds configured columns".into());
        }
        if style.text.is_some()
            && !matches!(
                node.kind,
                Kind::Label | Kind::Button(_) | Kind::Checkbox(..) | Kind::Radio(..)
            )
        {
            return Err("text Style applies to captions; TextBox font API/inherited text style is unavailable".into());
        }
        if style.text.is_some_and(|text| text.role == TextRole::Code) {
            return Err(
                "Code TextStyle requires unavailable public font_family API in Reactor 0.100.0"
                    .into(),
            );
        }
        if node.style.replace(style) != style {
            self.0.changed();
        }
        Ok(())
    }
    fn separator(&self, parent: &Parent, id: &str, vertical: bool) {
        if let Err(error) = self.insert(parent, id, Kind::Separator(vertical)) {
            self.report_error(error);
        }
        self.0.changed();
    }
}
impl TextView for View {
    fn label(&self, p: &Parent, id: &str, text: Binding<String>) {
        self.mount_text(p, id, text, false, None);
    }
    fn entry(&self, p: &Parent, id: &str, text: Binding<String>) {
        self.mount_text(p, id, text, true, None);
    }
    fn entry_with_placeholder(
        &self,
        p: &Parent,
        id: &str,
        text: Binding<String>,
        placeholder: Binding<String>,
    ) {
        self.mount_text(p, id, text, true, Some(placeholder));
    }
}
impl ButtonView for View {
    fn button(&self, parent: &Parent, id: &str, text: Binding<String>, command: Command) {
        match self.insert(parent, id, Kind::Button(command.clone())) {
            Ok(node) => {
                node.clicked.replace(Some(
                    self.event(&node, |view, node, ()| view.click(&node.id)),
                ));
                self.bind(&node, text, |_, node, value| {
                    node.text.replace(value);
                    Ok(())
                });
                self.bind(&node, command.enabled, |_, node, value| {
                    node.command_enabled.set(value);
                    Ok(())
                });
                self.0.changed();
            }
            Err(error) => self.report_error(error),
        }
    }
    fn dialog_footer(&self, parent: &Parent, id: &str, footer: DialogFooter) {
        let panel = self.layout(
            parent,
            id,
            Layout::Stack {
                horizontal: true,
                gap: 8.0,
            },
        );
        for (key, button) in footer.ordered() {
            self.button(
                &panel,
                &format!("{id}/{key}"),
                button.text.clone(),
                button.command.clone(),
            );
        }
        // Native buttons, not default-key accelerators: Return must respect IME.
    }
}
impl SelectView for View {
    fn checkbox(&self, parent: &Parent, id: &str, text: Binding<String>, checked: Binding<bool>) {
        match self.insert(
            parent,
            id,
            Kind::Checkbox(checked.clone(), Cell::new(false)),
        ) {
            Ok(node) => {
                node.checked_changed
                    .replace(Some(self.event(&node, |view, node, value| {
                        if let Kind::Checkbox(_, old) = &node.kind
                            && old.get() == value
                        {
                            return Ok(());
                        }
                        view.toggle(&node.id, value)
                    })));
                self.bind(&node, text, |_, node, value| {
                    node.text.replace(value);
                    Ok(())
                });
                self.bind(&node, checked, |_, node, value| {
                    if let Kind::Checkbox(_, old) = &node.kind {
                        old.set(value);
                    }
                    Ok(())
                });
            }
            Err(error) => self.report_error(error),
        }
    }
    fn radio(
        &self,
        parent: &Parent,
        id: &str,
        text: Binding<String>,
        selected: Binding<String>,
        value: &str,
    ) {
        if !panechord_runtime::valid_item_key(value) {
            self.report_error("invalid radio value".into());
            return;
        }
        match self.insert(
            parent,
            id,
            Kind::Radio(selected.clone(), value.into(), RefCell::default()),
        ) {
            Ok(node) => {
                node.checked_changed.replace(Some(self.event(
                    &node,
                    |view, node, checked: bool| {
                        if !checked {
                            return Ok(());
                        }
                        let Kind::Radio(_, value, old) = &node.kind else {
                            unreachable!()
                        };
                        if *old.borrow() == *value {
                            return Ok(());
                        }
                        view.select(&node.id, value)
                    },
                )));
                self.bind(&node, text, |_, node, value| {
                    node.text.replace(value);
                    Ok(())
                });
                self.bind(&node, selected, |_, node, value| {
                    if let Kind::Radio(_, _, old) = &node.kind {
                        old.replace(value);
                    }
                    Ok(())
                });
            }
            Err(error) => self.report_error(error),
        }
    }
    fn slider(&self, parent: &Parent, id: &str, value: Binding<f64>, range: SliderRange) {
        let initial = range.normalize(value.get()).map_err(str::to_owned);
        let Ok(initial) = initial else {
            self.report_error(initial.unwrap_err());
            return;
        };
        match self.insert(
            parent,
            id,
            Kind::Slider(value.clone(), range, Cell::new(initial)),
        ) {
            Ok(node) => {
                node.value_changed
                    .replace(Some(self.event(&node, |view, node, value| {
                        if let Kind::Slider(_, _, old) = &node.kind
                            && old.get() == value
                        {
                            return Ok(());
                        }
                        view.slide(&node.id, value)
                    })));
                self.bind(&node, value, |_, node, value| {
                    if let Kind::Slider(_, range, old) = &node.kind {
                        old.set(range.normalize(value).map_err(str::to_owned)?);
                    }
                    Ok(())
                });
            }
            Err(error) => self.report_error(error),
        }
    }
    fn combo(
        &self,
        parent: &Parent,
        id: &str,
        items: Binding<Vec<Choice>>,
        selected: Binding<String>,
    ) {
        if let Err(error) = panechord_runtime::validate_choices(&items.get(), &selected.get()) {
            self.report_error(error.into());
            return;
        }
        match self.insert(
            parent,
            id,
            Kind::Combo(
                selected.clone(),
                RefCell::default(),
                RefCell::new(selected.get()),
            ),
        ) {
            Ok(node) => {
                self.install_combo_callback(&node);
                self.bind(&node, items, |view, node, value| {
                    let Kind::Combo(_, choices, selected) = &node.kind else {
                        unreachable!()
                    };
                    panechord_runtime::validate_choices(&value, &selected.borrow())
                        .map_err(str::to_owned)?;
                    if *choices.borrow() != value {
                        choices.replace(value);
                        node.snapshot_revision
                            .set(node.snapshot_revision.get().wrapping_add(1));
                        view.install_combo_callback(node);
                    }
                    Ok(())
                });
                self.bind(&node, selected, |_, node, value| {
                    let Kind::Combo(_, choices, old) = &node.kind else {
                        unreachable!()
                    };
                    panechord_runtime::validate_choices(&choices.borrow(), &value)
                        .map_err(str::to_owned)?;
                    old.replace(value);
                    Ok(())
                });
            }
            Err(error) => self.report_error(error),
        }
    }
    fn slider_value(&self, id: &str) -> Option<f64> {
        self.find(id).ok().and_then(|node| {
            if let Kind::Slider(_, _, value) = &node.kind {
                Some(value.get())
            } else {
                None
            }
        })
    }
}
impl MediaView for View {
    fn image(&self, parent: &Parent, id: &str, source: Binding<Option<ImageFrame>>) {
        self.mount_image(parent, id, source, None);
    }
    fn image_with_pointer(
        &self,
        parent: &Parent,
        id: &str,
        source: Binding<Option<ImageFrame>>,
        pointer: Binding<Option<PointerPosition>>,
    ) {
        self.mount_image(parent, id, source, Some(pointer));
    }
    fn image_update_count(&self) -> usize {
        self.0.image_updates.get()
    }
    fn meter(&self, parent: &Parent, id: &str, value: Binding<f64>) {
        match self.insert(parent, id, Kind::Meter(Cell::new(0.0))) {
            Ok(node) => self.bind(&node, value, |_, node, value| {
                if !value.is_finite() {
                    return Err("meter value must be finite".into());
                }
                if let Kind::Meter(old) = &node.kind {
                    old.set(value.clamp(0.0, 1.0));
                }
                Ok(())
            }),
            Err(error) => self.report_error(error),
        }
    }
}
impl View {
    fn install_combo_callback(&self, node: &Rc<Node>) {
        let revision = node.snapshot_revision.get();
        node.selection_changed.replace(Some(self.event(
            node,
            move |view, node, index: Option<usize>| {
                if node.snapshot_revision.get() != revision {
                    return Ok(());
                }
                let Kind::Combo(_, choices, old) = &node.kind else {
                    unreachable!()
                };
                let value = match index {
                    Some(index) => {
                        let value = choices.borrow().get(index).map(|choice| choice.id.clone());
                        value.ok_or_else(|| {
                            view.recover(node);
                            "unknown choice index".to_owned()
                        })?
                    }
                    None => String::new(),
                };
                if *old.borrow() == value {
                    return Ok(());
                }
                view.select(&node.id, &value)
            },
        )));
    }
    fn mount_image(
        &self,
        parent: &Parent,
        id: &str,
        source: Binding<Option<ImageFrame>>,
        pointer: Option<Binding<Option<PointerPosition>>>,
    ) {
        match self.insert(
            parent,
            id,
            Kind::Image(RefCell::default(), RefCell::default(), pointer.clone()),
        ) {
            Ok(node) => {
                if pointer.is_some() {
                    node.pointer_moved.replace(Some(self.event(
                        &node,
                        |view, node, info: PointerEventInfo| {
                            view.set_pointer_node(
                                node,
                                Some(PointerPosition {
                                    x: info.x as f32,
                                    y: info.y as f32,
                                }),
                            )
                        },
                    )));
                    node.pointer_exited.replace(Some(
                        self.event(&node, |view, node, _: PointerEventInfo| {
                            view.set_pointer_node(node, None)
                        }),
                    ));
                }
                self.bind(&node, source, |view, node, value| {
                    let Kind::Image(old, bytes, _) = &node.kind else {
                        unreachable!()
                    };
                    if *old.borrow() == value {
                        return Ok(());
                    }
                    let encoded = value.as_ref().map(encode_image).transpose()?;
                    old.replace(value);
                    bytes.replace(encoded);
                    view.0.image_updates.set(view.0.image_updates.get() + 1);
                    Ok(())
                });
            }
            Err(error) => self.report_error(error),
        }
    }
    fn set_pointer_node(
        &self,
        node: &Rc<Node>,
        value: Option<PointerPosition>,
    ) -> Result<(), String> {
        if value.is_some_and(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return Err("pointer coordinates must be finite".into());
        }
        let Kind::Image(_, _, Some(binding)) = &node.kind else {
            return Err("image has no pointer binding".into());
        };
        self.write(node, binding, value)
    }
    pub fn set_pointer(&self, id: &str, value: Option<PointerPosition>) -> Result<(), String> {
        self.set_pointer_node(&self.find(id)?, value)
    }
}
impl AutomationView for View {
    fn edit(&self, id: &str, value: &str) -> Result<(), String> {
        let node = self.find(id)?;
        let Kind::Entry(binding) = &node.kind else {
            return Err("target is not an entry".into());
        };
        self.write(&node, binding, value.into())
    }
    fn click(&self, id: &str) -> Result<(), String> {
        let node = self.find(id)?;
        if let Kind::Page(key) = &node.kind {
            let parent = node
                .parent
                .as_ref()
                .and_then(|id| self.0.nodes.borrow().get(id).cloned())
                .ok_or("page has no tab")?;
            if !node.visible.get() || !node.enabled.get() {
                return Err("tab page is hidden/disabled".into());
            }
            return View(self.0.clone(), None).select(&parent.id, key);
        }
        if let Kind::TableCell(selection) = &node.kind {
            if !self.actionable(&node) {
                return Err("table cell or ancestor is hidden/disabled".into());
            }
            let parent = node.parent.as_ref().ok_or("cell has no table")?;
            return View(self.0.clone(), None).select_table(parent, Some(selection.clone()));
        }
        let Kind::Button(command) = &node.kind else {
            return Err("target is not a button".into());
        };
        if !self.actionable(&node) || !node.command_enabled.get() || !command.enabled.get() {
            return Err("button or command is hidden/disabled".into());
        }
        if command.invoke() {
            Ok(())
        } else {
            Err("command rejected invocation".into())
        }
    }
    fn toggle(&self, id: &str, value: bool) -> Result<(), String> {
        let node = self.find(id)?;
        let Kind::Checkbox(binding, _) = &node.kind else {
            return Err("target is not a checkbox".into());
        };
        self.write(&node, binding, value)
    }
    fn slide(&self, id: &str, value: f64) -> Result<(), String> {
        let node = self.find(id)?;
        let Kind::Slider(binding, range, _) = &node.kind else {
            return Err("target is not a slider".into());
        };
        let value = range.normalize(value).map_err(|error| {
            self.recover(&node);
            error.to_owned()
        })?;
        self.write(&node, binding, value)
    }
    fn select(&self, id: &str, value: &str) -> Result<(), String> {
        let node = self.find(id)?;
        match &node.kind {
            Kind::Combo(binding, choices, _) => {
                let validation = panechord_runtime::validate_choices(&choices.borrow(), value);
                validation.map_err(|error| {
                    self.recover(&node);
                    error.to_owned()
                })?;
                self.write(&node, binding, value.into())
            }
            Kind::Radio(binding, key, _) => {
                if key != value {
                    self.recover(&node);
                    return Err("unknown radio value".into());
                }
                self.write(&node, binding, value.into())
            }
            Kind::Tabs(tabs) => {
                if !self.tab_keys(&node).iter().any(|key| key == value) {
                    self.recover(&node);
                    return Err("unknown tab page".into());
                }
                self.write(&node, &tabs.selected, value.into())
            }
            _ => Err("target is not a selector".into()),
        }
    }
    fn set_open(&self, id: &str, value: bool) -> Result<(), String> {
        let node = self.find(id)?;
        let Kind::Dialog(binding, _) = &node.kind else {
            return Err("target is not a dialog".into());
        };
        // Opening a currently closed dialog is permitted; only its ancestors gate.
        if !node.visible.get() || !node.enabled.get() {
            return Err("dialog is hidden/disabled".into());
        }
        if let Some(parent) = &node.parent
            && let Some(parent) = self.0.nodes.borrow().get(parent).cloned()
            && !self.actionable(&parent)
        {
            return Err("dialog ancestor is hidden/disabled".into());
        }
        let result = binding.try_set(value).map_err(|error| error.to_string());
        if result.is_err() {
            self.recover(&node);
        }
        result
    }
    fn text(&self, id: &str) -> Option<String> {
        self.find(id).ok().and_then(|node| {
            matches!(
                node.kind,
                Kind::Label
                    | Kind::Entry(_)
                    | Kind::Button(_)
                    | Kind::Checkbox(..)
                    | Kind::Radio(..)
                    | Kind::Page(_)
                    | Kind::Menu(_)
                    | Kind::Dialog(..)
                    | Kind::TableCell(_)
            )
            .then(|| node.text.borrow().clone())
        })
    }
}

// Each mounted item owns its template's subscriptions and a weakly identified
// subtree. Neither an old Parent nor an old callback can name its replacement.
struct Mounted<T: Clone + 'static> {
    item: Keyed<T>,
    scope: Rc<Scope>,
    root: Rc<Node>,
    owner: WeakView,
}
impl<T: Clone + 'static> Drop for Mounted<T> {
    fn drop(&mut self) {
        self.scope.retire();
        if let Some(view) = self.owner.upgrade()
            && view.live(&self.root)
        {
            view.remove_tree(&self.root.id);
        }
    }
}
type TabTitle<T> = Rc<dyn Fn(Store<T>) -> Binding<String>>;
struct Collection<T: Clone + 'static> {
    owner: WeakView,
    node: Weak<Node>,
    rows: RefCell<Vec<Mounted<T>>>,
    snapshot: RefCell<Rc<Vec<Keyed<T>>>>,
    viewport: Cell<Option<Viewport>>,
    repeat: bool,
    template: Template<View, Parent, T>,
    title: Option<TabTitle<T>>,
    busy: Cell<bool>,
    pending: RefCell<Option<PendingCollection<T>>>,
}
impl<T: Clone + 'static> Collection<T> {
    fn submit(&self, items: Vec<Keyed<T>>, viewport: Option<Viewport>) -> Result<(), String> {
        let mut keys = BTreeSet::new();
        let limit = if self.repeat { MAX_MOUNTS } else { 16384 };
        if items.len() > limit
            || items.iter().any(|item| {
                !panechord_runtime::valid_item_key(&item.key)
                    || item.key.len() > 256
                    || item.key.contains('\0')
                    || !keys.insert(item.key.clone())
            })
        {
            return self.fail("invalid/duplicate collection keys or item limit".into());
        }
        if let Some(viewport) = viewport
            && let Err(error) = viewport.validate()
        {
            return self.fail(error.into());
        }
        let old = self.pending.replace(Some((items, viewport)));
        drop(old);
        let Some(_guard) = panechord_runtime::ReconcileGuard::enter(&self.busy) else {
            return Ok(());
        };
        for _ in 0..64 {
            let pending = self.pending.take();
            let Some((items, viewport)) = pending else {
                return Ok(());
            };
            let result = catch_unwind(AssertUnwindSafe(|| self.reconcile(items, viewport)));
            let result = match result {
                Ok(result) => result,
                Err(panic) => Err(format!(
                    "template reconciliation panicked: {}",
                    panic_message(&*panic)
                )),
            };
            if let Err(error) = result {
                // A failing application template cannot be rolled back after
                // arbitrary side effects. Retire the entire projection instead
                // of leaving stale keys/counts describing a partial tree.
                self.clear_projection();
                return self.fail(error);
            }
        }
        let old = self.pending.take();
        drop(old);
        self.fail("collection callback feedback limit".into())
    }
    fn fail(&self, error: String) -> Result<(), String> {
        if let Some(node) = self.node.upgrade() {
            node.item_error.replace(Some(error.clone()));
        }
        Err(error)
    }
    fn clear_projection(&self) {
        let rows = self.rows.take();
        let pending = self.pending.take();
        let snapshot = self.snapshot.replace(Rc::new(Vec::new()));
        drop(rows);
        drop(pending);
        drop(snapshot);
        if let Some(view) = self.owner.upgrade()
            && let Some(node) = self.node.upgrade().filter(|node| view.live(node))
        {
            let children = node.children.borrow().clone();
            for child in children {
                view.remove_tree(&child);
            }
            node.item_keys.borrow_mut().clear();
            view.0.changed();
        }
    }
    fn reconcile(&self, items: Vec<Keyed<T>>, viewport: Option<Viewport>) -> Result<(), String> {
        let Some(view) = self.owner.upgrade() else {
            return Ok(());
        };
        let Some(node) = self.node.upgrade().filter(|node| view.live(node)) else {
            return Ok(());
        };
        let range = match viewport {
            Some(vp) => vp.range(items.len()).map_err(str::to_owned)?,
            None => 0..items.len(),
        };
        let mut old = self.rows.take();
        let wanted = &items[range.clone()];
        // Retire every offscreen/replaced identity before mounting any new
        // template: even synchronous hooks never observe >256 mounted roots.
        let mut retained = Vec::new();
        for row in old {
            if wanted.contains(&row.item) {
                retained.push(row);
            } else {
                drop(row);
            }
        }
        old = retained;
        if !view.live(&node) {
            return Ok(());
        }
        // Only the selected viewport is mounted. Offscreen models remain owned
        // by the source snapshot; they are not closed when their widgets retire.
        let mut next = Vec::new();
        for item in &items[range] {
            if let Some(index) = old.iter().position(|row| row.item == *item) {
                let row = old.remove(index);
                if let Some(viewport) = viewport {
                    let mut style = row.root.style.get();
                    style.placement.height = Some(viewport.row_height);
                    row.root.style.set(style);
                }
                next.push(row);
                continue;
            }
            // Same key, different Store: retire the previous identity first.
            if let Some(index) = old.iter().position(|row| row.item.key == item.key) {
                drop(old.remove(index));
            }
            let prefix = format!("{}/{}", node.id, item.key);
            let kind = if self.title.is_some() {
                Kind::Page(item.key.clone())
            } else {
                Kind::Panel(Layout::default())
            };
            let root = view.insert(&view.parent_for(&node), &prefix, kind)?;
            if let Some(viewport) = viewport {
                root.style.set(Style {
                    placement: panechord_runtime::Placement {
                        height: Some(viewport.row_height),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            }
            let scope = Rc::new(Scope {
                prefix: prefix.clone(),
                alive: Cell::new(true),
                tokens: RefCell::default(),
            });
            let weak_scope = Rc::downgrade(&scope);
            root.cleanup.borrow_mut().push(Rc::new(move || {
                if let Some(scope) = weak_scope.upgrade() {
                    scope.retire();
                }
            }));
            let scoped = View(view.0.clone(), Some(scope.clone()));
            let mounted = Mounted {
                item: item.clone(),
                scope,
                root: root.clone(),
                owner: view.downgrade(),
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                if let Some(title) = &self.title {
                    scoped.install_page_callback(&root, &node);
                    let binding = title(item.model.clone());
                    scoped.bind(&root, binding, |_, node, value| {
                        node.text.replace(value);
                        Ok(())
                    });
                }
                (self.template)(
                    &scoped,
                    &scoped.parent_for(&root),
                    &prefix,
                    item.model.clone(),
                );
            }));
            if let Err(panic) = result {
                return Err(format!("template panicked: {}", panic_message(&*panic)));
            }
            if !view.live(&node) {
                return Ok(());
            }
            next.push(mounted);
        }
        drop(old);
        if !view.live(&node) {
            return Ok(());
        }
        node.children
            .replace(next.iter().map(|row| row.root.id.clone()).collect());
        let old = self.rows.replace(next);
        drop(old);
        node.item_keys
            .replace(items.iter().map(|item| item.key.clone()).collect());
        node.item_error.take();
        let old = self.snapshot.replace(Rc::new(items));
        drop(old);
        self.viewport.set(viewport);
        node.viewport_info.set(viewport);
        if let Kind::Tabs(tabs) = &node.kind {
            tabs.finished.set(true);
            view.normalize_tab(&node)?;
        }
        view.0.changed();
        Ok(())
    }
    fn move_viewport(&self, start: usize) {
        let Some(mut viewport) = self.viewport.get() else {
            return;
        };
        viewport.start = start;
        let snapshot = self.snapshot.borrow().clone();
        if let Err(error) = self.submit((*snapshot).clone(), Some(viewport))
            && let Some(view) = self.owner.upgrade()
        {
            view.report_error(error);
        }
    }
}
impl View {
    fn mount_collection<T: Clone + 'static>(
        &self,
        node: &Rc<Node>,
        items: Binding<Vec<Keyed<T>>>,
        viewport: Option<Binding<Viewport>>,
        repeat: bool,
        title: Option<TabTitle<T>>,
        template: Template<View, Parent, T>,
    ) {
        let initial_viewport = viewport.as_ref().map(Binding::get);
        let collection = Rc::new(Collection {
            owner: self.downgrade(),
            node: Rc::downgrade(node),
            rows: RefCell::default(),
            snapshot: RefCell::new(Rc::new(Vec::new())),
            viewport: Cell::new(initial_viewport),
            repeat,
            template,
            title,
            busy: Cell::new(false),
            pending: RefCell::default(),
        });
        node.dynamic.replace(Some(collection.clone()));
        let cleanup = Rc::downgrade(&collection);
        node.cleanup.borrow_mut().push(Rc::new(move || {
            if let Some(collection) = cleanup.upgrade() {
                let rows = collection.rows.take();
                let pending = collection.pending.take();
                let snapshot = collection.snapshot.replace(Rc::new(Vec::new()));
                collection.viewport.set(None);
                drop(rows);
                drop(pending);
                drop(snapshot);
            }
        }));
        let weak = Rc::downgrade(&collection);
        node.viewport_changed.replace(Some(Rc::new(move |start| {
            if let Some(collection) = weak.upgrade() {
                collection.move_viewport(start);
            }
        })));
        if viewport.is_some() {
            node.previous_page.replace(Some(
                self.event(node, |view, node, ()| view.page_collection(node, false)),
            ));
            node.next_page.replace(Some(
                self.event(node, |view, node, ()| view.page_collection(node, true)),
            ));
        }
        let state = Rc::downgrade(&collection);
        self.bind(node, items, move |_, _, items| {
            if let Some(state) = state.upgrade() {
                state.submit(items, state.viewport.get())
            } else {
                Ok(())
            }
        });
        if let Some(viewport) = viewport {
            let collection = Rc::downgrade(&collection);
            self.bind(node, viewport, move |_, _, viewport| {
                let Some(collection) = collection.upgrade() else {
                    return Ok(());
                };
                let snapshot = collection.snapshot.borrow().clone();
                collection.submit((*snapshot).clone(), Some(viewport))
            });
        }
    }
    fn page_collection(&self, node: &Rc<Node>, forward: bool) -> Result<(), String> {
        if !self.actionable(node) {
            return Err("list or ancestor is hidden/disabled".into());
        }
        let viewport = node.viewport_info.get().ok_or("list has no viewport")?;
        let start = viewport
            .range(node.item_keys.borrow().len())
            .map_err(str::to_owned)?
            .start;
        let start = if forward {
            start.saturating_add(viewport.count)
        } else {
            start.saturating_sub(viewport.count)
        };
        let callback = node
            .viewport_changed
            .borrow()
            .clone()
            .ok_or("list has no viewport callback")?;
        callback(start);
        Ok(())
    }
    fn render_scroll_content(&self, node: &Node, visible: bool, enabled: bool) -> ReactorView {
        let rows = self.stack_children(node, visible, enabled);
        let Some(viewport) = node.viewport_info.get() else {
            return rows;
        };
        let length = node.item_keys.borrow().len();
        let range = viewport.range(length).expect("validated viewport");
        // Public ScrollViewer has no offset-changed callback. Honest bounded
        // paging provides native access to every row, rather than claiming
        // automatic scroll-offset virtualization that the DSL cannot observe.
        let paging = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .keyed_children([
                KeyedView::new(
                    "previous",
                    Button::new()
                        .is_enabled(enabled && range.start > 0)
                        .on_click(
                            node.previous_page
                                .borrow()
                                .as_ref()
                                .expect("paging callback")
                                .clone(),
                        )
                        .content(TextBlock::new().text("Previous")),
                ),
                KeyedView::new(
                    "position",
                    TextBlock::new().text(format!(
                        "{}–{} / {}",
                        if length == 0 { 0 } else { range.start + 1 },
                        range.end,
                        length
                    )),
                ),
                KeyedView::new(
                    "next",
                    Button::new()
                        .is_enabled(enabled && range.end < length)
                        .on_click(
                            node.next_page
                                .borrow()
                                .as_ref()
                                .expect("paging callback")
                                .clone(),
                        )
                        .content(TextBlock::new().text("Next")),
                ),
            ]);
        StackPanel::new().keyed_children([
            KeyedView::new("paging", paging),
            KeyedView::new("rows", rows),
        ])
    }
}
impl ListView for View {
    fn repeat<T: Clone + 'static>(
        &self,
        parent: &Parent,
        id: &str,
        items: Binding<Vec<Keyed<T>>>,
        gap: f32,
        template: Template<View, Parent, T>,
    ) {
        let panel = self.layout(
            parent,
            id,
            Layout::Stack {
                horizontal: false,
                gap,
            },
        );
        if let Ok(node) = self.validate_parent(&panel) {
            self.mount_collection(&node, items, None, true, None, template);
        }
    }
    fn items<T: Clone + 'static>(
        &self,
        parent: &Parent,
        id: &str,
        items: Binding<Vec<Keyed<T>>>,
        viewport: Binding<Viewport>,
        template: Template<View, Parent, T>,
    ) {
        match self.insert(parent, id, Kind::Scroll(false)) {
            Ok(node) => self.mount_collection(&node, items, Some(viewport), false, None, template),
            Err(error) => self.report_error(error),
        }
    }
    fn content<T: Clone + 'static>(
        &self,
        parent: &Parent,
        id: &str,
        item: Binding<Option<Keyed<T>>>,
        template: Template<View, Parent, T>,
    ) {
        let panel = self.layout(parent, id, Layout::default());
        if let Ok(node) = self.validate_parent(&panel) {
            self.mount_collection(
                &node,
                item.map(|item| item.into_iter().collect()),
                None,
                true,
                None,
                template,
            );
        }
    }
    fn item_keys(&self, id: &str) -> Vec<String> {
        self.find(id)
            .map(|node| node.item_keys.borrow().clone())
            .unwrap_or_default()
    }
    fn mounted_count(&self, id: &str) -> usize {
        self.find(id)
            .map(|node| node.children.borrow().len())
            .unwrap_or(0)
    }
    fn viewport(&self, id: &str, start: usize) {
        if let Ok(node) = self.find(id) {
            let callback = node.viewport_changed.borrow().clone();
            if let Some(callback) = callback {
                callback(start);
            }
        }
    }
    fn items_error(&self, id: &str) -> Option<String> {
        self.find(id)
            .ok()
            .and_then(|node| node.item_error.borrow().clone())
    }
}
impl TabView for View {
    fn tab(&self, parent: &Parent, id: &str, selected: Binding<String>) -> Parent {
        match self.insert(
            parent,
            id,
            Kind::Tabs(TabState {
                selected: selected.clone(),
                value: RefCell::new(selected.get()),
                position: Cell::new(TabPosition::Top),
                finished: Cell::new(false),
            }),
        ) {
            Ok(node) => {
                self.bind(&node, selected, |view, node, value| {
                    let Kind::Tabs(tabs) = &node.kind else {
                        unreachable!()
                    };
                    tabs.value.replace(value);
                    if tabs.finished.get() {
                        view.normalize_tab(node)?;
                    }
                    Ok(())
                });
                self.0.changed();
                self.parent_for(&node)
            }
            Err(error) => {
                self.report_error(error);
                self.invalid_parent()
            }
        }
    }
    fn navigation(&self, parent: &Parent, id: &str, selected: Binding<String>) -> Parent {
        let tabs = self.tab(parent, id, selected);
        let _ = self.tab_position(id, TabPosition::Left);
        tabs
    }
    fn tab_position(&self, id: &str, position: TabPosition) -> Result<(), String> {
        let node = self.find(id)?;
        let Kind::Tabs(tabs) = &node.kind else {
            return Err("target is not a tab".into());
        };
        tabs.position.set(position);
        self.0.changed();
        Ok(())
    }
    fn tab_page(&self, parent: &Parent, id: &str, title: Binding<String>, key: &str) -> Parent {
        match self.try_tab_page(parent, id, title, key) {
            Ok(parent) => parent,
            Err(error) => {
                self.report_error(error);
                self.invalid_parent()
            }
        }
    }
    fn try_tab_page(
        &self,
        parent: &Parent,
        id: &str,
        title: Binding<String>,
        key: &str,
    ) -> Result<Parent, String> {
        let tab = self.validate_parent(parent)?;
        let Kind::Tabs(tabs) = &tab.kind else {
            return Err("TabPage requires a Tab parent".into());
        };
        if tabs.finished.get() {
            return Err("manual tab is already finished".into());
        }
        if !panechord_runtime::valid_item_key(key) || key.len() > 256 || key.contains('\0') {
            return Err("invalid tab page key".into());
        }
        if self.tab_keys(&tab).iter().any(|old| old == key) {
            return Err("duplicate tab page key".into());
        }
        if tab.children.borrow().len() >= MAX_MOUNTS {
            return Err("tab page limit".into());
        }
        let page = self.insert(parent, id, Kind::Page(key.into()))?;
        self.install_page_callback(&page, &tab);
        self.bind(&page, title, |_, node, value| {
            node.text.replace(value);
            Ok(())
        });
        Ok(self.parent_for(&page))
    }
    fn finish_tab(&self, parent: &Parent) -> Result<(), String> {
        let node = self.validate_parent(parent)?;
        let Kind::Tabs(tabs) = &node.kind else {
            return Err("target is not a tab".into());
        };
        tabs.finished.set(true);
        self.normalize_tab(&node)?;
        self.0.changed();
        Ok(())
    }
    fn tab_items<T: Clone + 'static>(
        &self,
        parent: &Parent,
        id: &str,
        items: Binding<Vec<Keyed<T>>>,
        selected: Binding<String>,
        title: TabTitle<T>,
        template: Template<View, Parent, T>,
    ) -> Parent {
        let tabs = self.tab(parent, id, selected);
        if let Ok(node) = self.validate_parent(&tabs) {
            self.mount_collection(&node, items, None, true, Some(title), template);
        }
        tabs
    }
}
impl View {
    fn tab_keys(&self, node: &Node) -> Vec<String> {
        let children = node.children.borrow().clone();
        children
            .iter()
            .filter_map(|id| {
                self.0.nodes.borrow().get(id).and_then(|node| {
                    if let Kind::Page(key) = &node.kind {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
    fn normalize_tab(&self, node: &Rc<Node>) -> Result<(), String> {
        let Kind::Tabs(tabs) = &node.kind else {
            return Err("target is not a tab".into());
        };
        let keys = self.tab_keys(node);
        let current = tabs.value.borrow().clone();
        if keys.contains(&current) || (keys.is_empty() && current.is_empty()) {
            return Ok(());
        }
        let next = keys.first().cloned().unwrap_or_default();
        tabs.value.replace(next.clone());
        // Readonly bindings still display a deterministic first-page fallback.
        // A rejecting writer does not leave an impossible native selection.
        if tabs.selected.is_writable() {
            tabs.selected
                .try_set(next)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    fn install_page_callback(&self, page: &Rc<Node>, tab: &Rc<Node>) {
        let tab = Rc::downgrade(tab);
        page.clicked
            .replace(Some(self.event(page, move |view, page, ()| {
                let tab = tab
                    .upgrade()
                    .filter(|tab| view.live(tab))
                    .ok_or("stale tab")?;
                if !page.visible.get() || !page.enabled.get() {
                    return Err("tab page is hidden/disabled".into());
                }
                let Kind::Page(key) = &page.kind else {
                    unreachable!()
                };
                View(view.0.clone(), None).select(&tab.id, key)
            })));
    }
    fn render_tabs(
        &self,
        node: &Node,
        tabs: &TabState,
        visible: bool,
        enabled: bool,
    ) -> ReactorView {
        let children = node.children.borrow().clone();
        let vertical = matches!(tabs.position.get(), TabPosition::Left | TabPosition::Right);
        let mut headers = Vec::new();
        for id in &children {
            let page = self.0.nodes.borrow().get(id).cloned().expect("page");
            let Kind::Page(key) = &page.kind else {
                continue;
            };
            let button = Button::new()
                .is_enabled(
                    enabled
                        && page.visible.get()
                        && page.enabled.get()
                        && tabs.selected.is_writable(),
                )
                .style(if *tabs.value.borrow() == *key {
                    windows_reactor::ButtonStyle::Accent
                } else {
                    windows_reactor::ButtonStyle::Default
                })
                .on_click(
                    page.clicked
                        .borrow()
                        .as_ref()
                        .expect("tab callback")
                        .clone(),
                )
                .content(styled_text(&page));
            headers.push(KeyedView::new(id.clone(), button));
        }
        let header: ReactorView = StackPanel::new()
            .orientation(if vertical {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            })
            .keyed_children(headers);
        let pages: ReactorView = Grid::new().keyed_children(
            children
                .iter()
                .map(|id| KeyedView::new(id.clone(), self.render_node(id, visible, enabled))),
        );
        let reverse = matches!(
            tabs.position.get(),
            TabPosition::Bottom | TabPosition::Right
        );
        let (h, p) = if reverse { (1, 0) } else { (0, 1) };
        let mut grid = Grid::new();
        if vertical {
            grid = grid.columns(if reverse {
                vec![GridLength::STAR, GridLength::Auto]
            } else {
                vec![GridLength::Auto, GridLength::STAR]
            });
            grid.keyed_children([
                KeyedView::new("headers", Grid::new().grid_column(h).children((header,))),
                KeyedView::new("pages", Grid::new().grid_column(p).children((pages,))),
            ])
        } else {
            grid = grid.rows(if reverse {
                vec![GridLength::STAR, GridLength::Auto]
            } else {
                vec![GridLength::Auto, GridLength::STAR]
            });
            grid.keyed_children([
                KeyedView::new("headers", Grid::new().grid_row(h).children((header,))),
                KeyedView::new("pages", Grid::new().grid_row(p).children((pages,))),
            ])
        }
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
        match self.insert(parent, id, Kind::Dialog(open.clone(), Cell::new(false))) {
            Ok(node) => {
                node.dialog_closed.replace(Some(
                    self.event(&node, |view, node, _| view.set_open(&node.id, false)),
                ));
                self.bind(&node, title, |_, node, value| {
                    node.text.replace(value);
                    Ok(())
                });
                self.bind(&node, open, |_, node, value| {
                    if let Kind::Dialog(_, old) = &node.kind {
                        old.set(value);
                    }
                    Ok(())
                });
                self.parent_for(&node)
            }
            Err(error) => {
                self.report_error(error);
                self.invalid_parent()
            }
        }
    }
}
impl TableView for View {
    fn table(&self, parent: &Parent, id: &str, data: Binding<Table>) {
        self.mount_table(parent, id, data, None);
    }
    fn table_with_selection(
        &self,
        parent: &Parent,
        id: &str,
        data: Binding<Table>,
        selection: Binding<Option<TableSelection>>,
    ) {
        self.mount_table(parent, id, data, Some(selection));
    }
}
impl View {
    fn mount_table(
        &self,
        parent: &Parent,
        id: &str,
        data: Binding<Table>,
        selection: Option<Binding<Option<TableSelection>>>,
    ) {
        match self.insert(
            parent,
            id,
            Kind::Table(TableState {
                data: RefCell::default(),
                selected: RefCell::default(),
                selection: selection.clone(),
                cells: RefCell::default(),
            }),
        ) {
            Ok(node) => {
                self.bind(&node, data, |view, node, data| {
                    view.update_table(node, data)
                });
                if let Some(selection) = selection {
                    self.bind(&node, selection, |_, node, value| {
                        let Kind::Table(table) = &node.kind else {
                            unreachable!()
                        };
                        if let Some(selection) = &value {
                            let data = table.data.borrow();
                            selection
                                .validate(data.as_ref().ok_or("table has no valid snapshot")?)
                                .map_err(str::to_owned)?;
                        }
                        table.selected.replace(value);
                        Ok(())
                    });
                }
            }
            Err(error) => self.report_error(error),
        }
    }
    fn update_table(&self, node: &Rc<Node>, data: Table) -> Result<(), String> {
        data.validate().map_err(str::to_owned)?;
        if data
            .columns
            .iter()
            .any(|column| column.overflow == TextOverflow::EllipsizeMiddle)
        {
            return Err("middle ellipsis is not exposed by native TextTrimming".into());
        }
        if data
            .columns
            .iter()
            .any(|column| column.text.role == TextRole::Code)
        {
            return Err("Code TextStyle requires unavailable public font_family API in Reactor 0.100.0; table snapshot rejected".into());
        }
        let Kind::Table(table) = &node.kind else {
            unreachable!()
        };
        if table.data.borrow().as_ref() == Some(&data) {
            return Ok(());
        }
        let limit = MAX_MOUNTS.min(4096 / data.columns.len());
        if data.rows.len() > limit {
            return Err(format!(
                "table row budget {limit} exceeded; snapshot rejected, not truncated"
            ));
        }
        let rows = data.rows.len();
        let old_count = table.cells.borrow().len();
        if self.0.nodes.borrow().len().saturating_sub(old_count) + rows * data.columns.len()
            > MAX_NODES
        {
            return Err("table node budget".into());
        }
        if node.depth >= MAX_DEPTH {
            return Err("table depth limit".into());
        }
        let mut cells = BTreeMap::new();
        for row in data.rows.iter().take(rows) {
            for (index, cell) in row.cells.iter().enumerate() {
                if data.columns[index].hide_empty && cell.text.is_empty() {
                    continue;
                }
                let id = format!("{}/{}/{}", node.id, row.key, index);
                if id.len() > 2048 || id.contains('\0') {
                    return Err("invalid table cell id".into());
                }
                {
                    let nodes = self.0.nodes.borrow();
                    let old = table.cells.borrow();
                    if let Some(existing) = nodes.get(&id)
                        && !old.get(&id).is_some_and(|old| Rc::ptr_eq(old, existing))
                    {
                        return Err(format!("table cell id collides with existing node: {id}"));
                    }
                }
                let selection = TableSelection {
                    row_key: row.key.clone(),
                    column: index,
                };
                let child = Node::new(
                    id.clone(),
                    Some(node.id.clone()),
                    node.depth + 1,
                    Kind::TableCell(selection),
                );
                child.text.replace(cell.text.clone());
                let mut style = data.columns[index].text;
                style.foreground = cell.foreground.or(style.foreground);
                child.style.set(Style {
                    text: Some(style),
                    ..Default::default()
                });
                child.clicked.replace(Some(
                    self.event(&child, |view, cell, ()| view.click(&cell.id)),
                ));
                cells.insert(id, child);
            }
        }
        // Prepare everything before touching the live registry. Commit has no
        // application callbacks; only then retire old subscriptions, so their
        // teardown hooks observe a coherent new table and can safely reenter.
        let old = table.cells.replace(cells.clone());
        let mut removed = Vec::new();
        {
            let mut nodes = self.0.nodes.borrow_mut();
            for (id, child) in &old {
                child.alive.set(false);
                if nodes
                    .get(id)
                    .is_some_and(|current| Rc::ptr_eq(current, child))
                    && let Some(child) = nodes.remove(id)
                {
                    removed.push(child);
                }
            }
            for (id, child) in &cells {
                nodes.insert(id.clone(), child.clone());
            }
        }
        let mut children = node.children.borrow().clone();
        children.retain(|id| !old.contains_key(id));
        children.extend(cells.keys().cloned());
        node.children.replace(children);
        let selected = table.selected.borrow().clone();
        if selected
            .as_ref()
            .is_some_and(|selection| selection.validate(&data).is_err())
        {
            table.selected.take();
        }
        table.data.replace(Some(data));
        for child in old.into_values() {
            self.0.attempt(|| child.retire());
        }
        drop(removed);
        Ok(())
    }
    pub fn table_snapshot(&self, id: &str) -> Option<Table> {
        self.find(id).ok().and_then(|node| {
            if let Kind::Table(table) = &node.kind {
                table.data.borrow().clone()
            } else {
                None
            }
        })
    }
    pub fn table_selection(&self, id: &str) -> Option<TableSelection> {
        self.find(id).ok().and_then(|node| {
            if let Kind::Table(table) = &node.kind {
                table.selected.borrow().clone()
            } else {
                None
            }
        })
    }
    pub fn select_table(&self, id: &str, selection: Option<TableSelection>) -> Result<(), String> {
        let node = self.find(id)?;
        let Kind::Table(table) = &node.kind else {
            return Err("target is not a table".into());
        };
        if let Some(selection) = &selection {
            let data = table
                .data
                .borrow()
                .clone()
                .ok_or("table has no valid snapshot")?;
            selection.validate(&data).map_err(str::to_owned)?;
        }
        let binding = table
            .selection
            .as_ref()
            .ok_or("table selection is read-only")?;
        self.write(&node, binding, selection)
    }
    fn render_table(
        &self,
        node: &Node,
        table: &TableState,
        visible: bool,
        enabled: bool,
    ) -> ReactorView {
        let data = table.data.borrow().clone();
        let Some(data) = data else {
            return TextBlock::new().text("Invalid table").into();
        };
        let mut rows = Vec::new();
        let columns: Vec<_> = data
            .columns
            .iter()
            .map(|column| match column.width {
                Some(width) => GridLength::Pixel(f64::from(width)),
                None if column.stretch => GridLength::STAR,
                None => GridLength::Auto,
            })
            .collect();
        rows.push(KeyedView::new(
            "headers",
            Grid::new()
                .columns(columns.clone())
                .column_spacing(f64::from(data.column_gap))
                .keyed_children(data.columns.iter().enumerate().map(|(index, column)| {
                    KeyedView::new(
                        index,
                        Grid::new()
                            .grid_column(index as i32)
                            .children((text_style(&column.title, column.text),)),
                    )
                })),
        ));
        if data.rows.is_empty() {
            rows.push(KeyedView::new(
                "empty",
                TextBlock::new().text(&data.empty_text),
            ));
        }
        let cells = table.cells.borrow().clone();
        for row in data
            .rows
            .iter()
            .take(MAX_MOUNTS)
            .take(4096 / data.columns.len())
        {
            let mut children = Vec::new();
            for (index, cell) in row.cells.iter().enumerate() {
                let id = format!("{}/{}/{}", node.id, row.key, index);
                let Some(child) = cells.get(&id) else {
                    continue;
                };
                let mut caption =
                    text_style(&cell.text, child.style.get().text.unwrap_or_default())
                        .text_trimming(TextTrimming::Clip)
                        .text_wrapping(if data.columns[index].overflow == TextOverflow::Wrap {
                            TextWrapping::Wrap
                        } else {
                            TextWrapping::NoWrap
                        });
                if let Some([r, g, b]) = cell.foreground {
                    caption = caption.foreground(Color::rgb(r, g, b));
                }
                let content: ReactorView = if table.selection.is_some() {
                    let selected = table
                        .selected
                        .borrow()
                        .as_ref()
                        .is_some_and(|s| s.row_key == row.key && s.column == index);
                    Button::new()
                        .is_enabled(
                            enabled && table.selection.as_ref().is_some_and(Binding::is_writable),
                        )
                        .style(if selected {
                            windows_reactor::ButtonStyle::Accent
                        } else {
                            windows_reactor::ButtonStyle::Default
                        })
                        .on_click(
                            child
                                .clicked
                                .borrow()
                                .as_ref()
                                .expect("table callback")
                                .clone(),
                        )
                        .content(caption)
                } else {
                    caption.into()
                };
                children.push(KeyedView::new(
                    index,
                    Grid::new().grid_column(index as i32).children((content,)),
                ));
            }
            rows.push(KeyedView::new(
                row.key.clone(),
                Border::new()
                    .padding(Thickness::uniform(f64::from(data.row_inset)))
                    .content(
                        Grid::new()
                            .columns(columns.clone())
                            .column_spacing(f64::from(data.column_gap))
                            .keyed_children(children),
                    ),
            ));
        }
        let _ = visible;
        ScrollViewer::new()
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .content(
                StackPanel::new()
                    .spacing(f64::from(data.row_gap))
                    .keyed_children(rows),
            )
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
        self.mount_menu(
            id,
            title,
            items.map(|items| items.into_iter().map(MenuNode::Item).collect()),
            dispatch,
        );
    }
    fn hierarchical_menu<A: Clone + PartialEq + 'static>(
        &self,
        id: &str,
        title: Binding<String>,
        items: Binding<Vec<MenuNode<A>>>,
        dispatch: Rc<dyn Fn(A) -> bool>,
    ) {
        self.mount_menu(id, title, items, dispatch);
    }
    fn activate_menu(&self, id: &str) -> Result<(), String> {
        if self.is_closed() {
            return Err("view or template scope is closed".into());
        }
        let nodes: Vec<_> = self.0.nodes.borrow().values().cloned().collect();
        let mut matches = Vec::new();
        for node in nodes {
            if let Kind::Menu(menu) = &node.kind
                && menu.actions.borrow().contains_key(id)
            {
                matches.push(node.clone());
            }
        }
        match matches.as_slice() {
            [node] => self.activate_menu_node(node, id),
            [] => Err(format!("unknown menu item: {id}")),
            _ => Err(format!("ambiguous menu item across menus: {id}")),
        }
    }
}
type MenuActions = BTreeMap<String, Rc<dyn Fn() -> bool>>;
impl View {
    fn mount_menu<A: Clone + 'static>(
        &self,
        id: &str,
        title: Binding<String>,
        source: Binding<Vec<MenuNode<A>>>,
        dispatch: Rc<dyn Fn(A) -> bool>,
    ) {
        match self.insert(
            &self.root(),
            id,
            Kind::Menu(MenuState {
                native: RefCell::default(),
                native_ids: RefCell::default(),
                actions: RefCell::default(),
                metadata: RefCell::default(),
                callback: RefCell::default(),
            }),
        ) {
            Ok(node) => {
                let Kind::Menu(menu) = &node.kind else {
                    unreachable!()
                };
                menu.callback
                    .replace(Some(self.event(&node, |_, _, _: String| {
                        Err("menu has no valid snapshot".into())
                    })));
                self.bind(&node, title, |_, node, title| {
                    node.text.replace(title);
                    Ok(())
                });
                self.bind(&node, source, move |view, node, items| {
                    let leaves = panechord_runtime::flatten_menu(&items)?;
                    // Validate the full snapshot, including invisible leaves, before commit.
                    for leaf in &leaves {
                        if let Some(raw) = &leaf.shortcut {
                            let chord = parse_shortcut(raw)?;
                            if !matches!(leaf.role, MenuRole::Normal | MenuRole::Quit) {
                                return Err("custom shortcut on predefined native role is unsupported".into());
                            }
                            owned_render::shortcut_native(&chord)?;
                        }
                    }
                    let items = panechord_runtime::visible_tree(&items);
                    // 0.100.0 delivers the label, not Key. Reject ambiguous
                    // labels before commit; never guess which argument to call.
                    let mut labels = BTreeMap::new();
                    for leaf in panechord_runtime::flatten_menu(&items)? {
                        if leaf.role != MenuRole::Separator && labels.insert(leaf.label.clone(), leaf.id.clone()).is_some() {
                            return Err("duplicate visible native menu labels are ambiguous in Reactor 0.100.0".into());
                        }
                    }
                    let mut actions = BTreeMap::new();
                    let mut metadata = BTreeMap::new();
                    let native = build_menu(&items, true, &dispatch, &mut actions, &mut metadata);
                    let mut chords = BTreeSet::new();
                    let mut existing_chords = BTreeSet::new();
                    for other in view.0.nodes.borrow().values() {
                        if Rc::ptr_eq(other, node) { continue; }
                        if let Kind::Menu(other) = &other.kind {
                            for old in other.metadata.borrow().values() {
                                if let Some(old) = &old.chord {
                                    let (key, modifiers) = owned_render::shortcut_native(old)?;
                                    existing_chords.insert((key.0, modifiers.0));
                                }
                            }
                        }
                    }
                    for entry in metadata.values() {
                        if let Some(chord) = &entry.chord {
                            let (key, modifiers) = owned_render::shortcut_native(chord)?;
                            if !chords.insert((key.0, modifiers.0)) {
                                return Err("duplicate native menu shortcut".into());
                            }
                            // Root accelerators share the window; reject ambiguity across menus too.
                            if existing_chords.contains(&(key.0, modifiers.0)) {
                                return Err("ambiguous native menu shortcut across menus".into());
                            }
                        }
                    }
                    let Kind::Menu(menu) = &node.kind else { unreachable!() };
                    let revision = node.snapshot_revision.get().checked_add(1).ok_or("menu snapshot revision exhausted")?;
                    let native_ids = metadata.keys().map(|id| (windows_reactor::Key::from(id.clone()), id.clone())).collect();
                    let old = menu.actions.replace(actions); menu.native.replace(native); menu.metadata.replace(metadata);
                    menu.native_ids.replace(native_ids);
                    node.snapshot_revision.set(revision);
                    menu.callback.replace(Some(view.event(node, move |view, node, label: String| {
                        if node.snapshot_revision.get() != revision { return Ok(()); }
                        let id = labels.get(&label).ok_or("unknown native menu label")?;
                        view.activate_menu_node(node, id)
                    }))); drop(old); Ok(())
                });
            }
            Err(error) => self.report_error(error),
        }
    }
    fn activate_menu_node(&self, node: &Rc<Node>, id: &str) -> Result<(), String> {
        let Kind::Menu(menu) = &node.kind else {
            return Err("target is not a menu".into());
        };
        if !self.actionable(node) {
            return Err("menu or ancestor is hidden/disabled".into());
        }
        let metadata = menu
            .metadata
            .borrow()
            .get(id)
            .cloned()
            .ok_or("unknown menu item metadata")?;
        if !metadata.enabled {
            return Err("menu command is disabled".into());
        }
        match metadata.role {
            MenuRole::Copy | MenuRole::Cut | MenuRole::Paste | MenuRole::SelectAll => {
                let route = self
                    .0
                    .native_edit
                    .borrow()
                    .clone()
                    .ok_or("no focused native editor (owned host required)")?;
                return route(metadata.role);
            }
            MenuRole::Quit => {
                self.request_close();
                return Ok(());
            }
            MenuRole::Normal | MenuRole::About => (),
            MenuRole::Separator => return Err("separator is not actionable".into()),
        }
        let action = menu
            .actions
            .borrow()
            .get(id)
            .cloned()
            .ok_or_else(|| format!("unknown menu item: {id}"))?;
        if action() {
            Ok(())
        } else {
            Err("menu command is disabled or rejected".into())
        }
    }
}
fn parse_shortcut(raw: &str) -> Result<panechord_runtime::Shortcut, String> {
    panechord_runtime::Shortcut::parse(raw)
        .map_err(|error| format!("invalid menu shortcut {raw:?}: {error}"))
}
fn build_menu<A: Clone + 'static>(
    items: &[MenuNode<A>],
    enabled: bool,
    dispatch: &Rc<dyn Fn(A) -> bool>,
    actions: &mut MenuActions,
    metadata: &mut BTreeMap<String, MenuMetadata>,
) -> Vec<windows_reactor::MenuItem> {
    let mut native = Vec::new();
    for item in items {
        match item {
            MenuNode::Item(item) if item.role == MenuRole::Separator => {
                native.push(windows_reactor::MenuItem::separator(item.id.clone()))
            }
            MenuNode::Item(item) => {
                let allowed = enabled && item.enabled;
                metadata.insert(
                    item.id.clone(),
                    MenuMetadata {
                        role: item.role.clone(),
                        chord: item
                            .shortcut
                            .as_deref()
                            .map(parse_shortcut)
                            .transpose()
                            .expect("validated shortcut"),
                        enabled: allowed,
                    },
                );
                let dispatch = dispatch.clone();
                let argument = item.argument.clone();
                actions.insert(
                    item.id.clone(),
                    Rc::new(move || allowed && dispatch(argument.clone())),
                );
                native.push(if allowed {
                    windows_reactor::MenuItem::item(item.id.clone(), item.label.clone())
                } else {
                    windows_reactor::MenuItem::disabled(item.id.clone(), item.label.clone())
                });
            }
            MenuNode::Submenu {
                id,
                label,
                enabled: local,
                children,
            } => {
                metadata.insert(
                    id.clone(),
                    MenuMetadata {
                        role: MenuRole::Normal,
                        chord: None,
                        enabled: enabled && *local,
                    },
                );
                native.push(windows_reactor::MenuItem::submenu(
                    id.clone(),
                    label.clone(),
                    build_menu(children, enabled && *local, dispatch, actions, metadata),
                ));
            }
        }
    }
    native
}
fn disabled_menu(items: &[windows_reactor::MenuItem]) -> Vec<windows_reactor::MenuItem> {
    items
        .iter()
        .map(|item| match item {
            windows_reactor::MenuItem::Item { key, label, .. } => {
                windows_reactor::MenuItem::disabled(key.clone(), label.clone())
            }
            windows_reactor::MenuItem::Separator { key } => {
                windows_reactor::MenuItem::separator(key.clone())
            }
            windows_reactor::MenuItem::Submenu { key, label, items } => {
                windows_reactor::MenuItem::submenu(key.clone(), label.clone(), disabled_menu(items))
            }
        })
        .collect()
}
