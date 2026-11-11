//! Direct native projection of the retained registry; never invokes Reactor DSL.
use super::{Kind, Node, ROOT, View};
use crate::{
    driver::{callback, sdk_error, with_renderer},
    executor::Bridge,
    owned_bindings as n,
};
use futures::{
    FutureExt,
    future::{AbortHandle, Abortable},
};
use panechord_runtime::{
    Alignment, AutomationView, Edges, Executor, Layout, MenuView, SelectView, TabPosition,
    TextOverflow, TextRole, TextStyle, ViewCore,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use windows_core::{EventRevoker, IInspectable, Interface, Result};

#[path = "owned_menu.rs"]
mod owned_menu;
pub(super) use owned_menu::shortcut_native;

static NEXT_MENU_EPOCH: AtomicU64 = AtomicU64::new(0);

fn next_menu_epoch() -> Result<u64> {
    let mut previous = NEXT_MENU_EPOCH.load(Ordering::Relaxed);
    loop {
        let next = previous
            .checked_add(1)
            .ok_or_else(|| sdk_error("menu renderer epoch exhausted"))?;
        match NEXT_MENU_EPOCH.compare_exchange_weak(
            previous,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Ok(next),
            Err(current) => previous = current,
        }
    }
}

struct Accelerator {
    native: n::KeyboardAccelerator,
    revision: u64,
    _revoker: EventRevoker,
}

type Key = (usize, String);
struct Object {
    token: u64,
    node: Rc<Node>,
    role: String,
    element: n::UIElement,
    revokers: RefCell<Vec<EventRevoker>>,
    abort: RefCell<Vec<AbortHandle>>,
    children: RefCell<Vec<n::UIElement>>,
    desired: RefCell<Vec<n::UIElement>>,
    foreground: Option<n::Brush>,
    background: Option<n::Brush>,
    loaded: Cell<bool>,
    image: RefCell<Option<Vec<u8>>>,
    revision: Cell<u64>,
    operation: RefCell<Option<windows_future::IAsyncOperation<n::ContentDialogResult>>>,
    showing: Cell<bool>,
    changes: Cell<u64>,
    menu_event: RefCell<Option<EventRevoker>>,
    menu_id: RefCell<Option<String>>,
    image_revision: Cell<u64>,
}
impl Object {
    fn retire(&self) -> Result<()> {
        self.revokers.take();
        self.menu_event.take();
        for abort in self.abort.take() {
            abort.abort();
        }
        if self.showing.replace(false) {
            self.element.cast::<n::IContentDialog>()?.Hide()?;
        }
        self.operation.take();
        Ok(())
    }
}
impl Drop for Object {
    fn drop(&mut self) {
        self.revokers.get_mut().clear();
        for abort in self.abort.get_mut().drain(..) {
            abort.abort();
        }
    }
}
pub(crate) struct Renderer {
    view: View,
    generation: u64,
    menu_epoch: u64,
    bridge: Rc<Bridge>,
    objects: RefCell<BTreeMap<Key, Rc<Object>>>,
    used: RefCell<BTreeSet<Key>>,
    next: Cell<u64>,
    applying: Cell<bool>,
    retired: Cell<bool>,
    stale_entry: RefCell<Option<(u64, n::ITextBox)>>,
    last_focused_entry: Cell<Option<u64>>,
    accelerators: RefCell<BTreeMap<u64, Accelerator>>,
    accelerators_revoked: Cell<u64>,
    last_revoked_accelerator: RefCell<Option<(u64, u64, n::KeyboardAccelerator)>>,
}
impl Renderer {
    pub(crate) fn new(view: View, generation: u64, bridge: Rc<Bridge>) -> Result<Self> {
        let menu_epoch = next_menu_epoch()?;
        let owner = view.downgrade();
        view.0.native_edit.replace(Some(Rc::new(move |role| {
            let owner = owner.upgrade().ok_or("native editor owner retired")?;
            let mut result = Err("no focused native editor (owned host required)".into());
            with_renderer(generation, |r| {
                if r.menu_epoch == menu_epoch && r.view.same_view(&owner) {
                    result = r.edit_menu(role);
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            result
        })));
        Ok(Self {
            view,
            generation,
            menu_epoch,
            bridge,
            objects: RefCell::default(),
            used: RefCell::default(),
            next: Cell::new(0),
            applying: Cell::new(false),
            retired: Cell::new(false),
            stale_entry: RefCell::default(),
            last_focused_entry: Cell::new(None),
            accelerators: RefCell::default(),
            accelerators_revoked: Cell::new(0),
            last_revoked_accelerator: RefCell::default(),
        })
    }
    fn key(node: &Rc<Node>, role: &str) -> Key {
        (Rc::as_ptr(node) as usize, role.into())
    }
    fn object(
        &self,
        node: &Rc<Node>,
        role: &str,
        create: impl FnOnce() -> Result<n::UIElement>,
    ) -> Result<Rc<Object>> {
        let key = Self::key(node, role);
        self.used.borrow_mut().insert(key.clone());
        if let Some(object) = self.objects.borrow().get(&key).cloned() {
            return Ok(object);
        }
        if self.objects.borrow().len() >= super::MAX_NODES * 8 {
            return Err(sdk_error("native synthetic object budget"));
        }
        let token = self
            .next
            .get()
            .checked_add(1)
            .ok_or_else(|| sdk_error("native event token exhausted"))?;
        self.next.set(token);
        let element = create()?;
        let foreground = if let Ok(text) = element.cast::<n::ITextBlock>() {
            optional(text.Foreground())?
        } else {
            None
        };
        let background = if let Ok(control) = element.cast::<n::IControl>() {
            optional(control.Background())?
        } else {
            None
        };
        let object = Rc::new(Object {
            token,
            node: node.clone(),
            role: role.into(),
            element,
            revokers: RefCell::default(),
            abort: RefCell::default(),
            children: RefCell::default(),
            desired: RefCell::default(),
            foreground,
            background,
            loaded: Cell::new(false),
            image: RefCell::default(),
            revision: Cell::new(node.snapshot_revision.get()),
            operation: RefCell::default(),
            showing: Cell::new(false),
            changes: Cell::new(0),
            menu_event: RefCell::default(),
            menu_id: RefCell::default(),
            image_revision: Cell::new(0),
        });
        self.objects.borrow_mut().insert(key, object.clone());
        if role == "control" || role == "header" || role == "previous" || role == "next" {
            self.events(&object)?;
        }
        let generation = self.generation;
        object
            .revokers
            .borrow_mut()
            .push(
                object
                    .element
                    .cast::<n::IFrameworkElement>()?
                    .Loaded(move |_, _| {
                        callback(|| {
                            with_renderer(generation, |r| {
                                let o = r
                                    .objects
                                    .borrow()
                                    .values()
                                    .find(|o| o.token == token)
                                    .cloned();
                                if let Some(o) = o {
                                    o.loaded.set(true);
                                }
                                Ok(())
                            })
                        })
                    })?,
            );
        Ok(object)
    }
    fn panel(&self, node: &Rc<Node>, role: &str, grid: bool) -> Result<Rc<Object>> {
        self.object(node, role, || {
            if grid {
                n::Grid::new()?.cast()
            } else {
                n::StackPanel::new()?.cast()
            }
        })
    }
    fn child_list(&self, object: &Object, children: Vec<n::UIElement>) {
        object.desired.replace(children);
    }
    fn content(&self, object: &Object, content: &n::UIElement) -> Result<()> {
        let desired = vec![content.clone()];
        if *object.children.borrow() == desired {
            return Ok(());
        }
        if let Ok(border) = object.element.cast::<n::IBorder>() {
            border.SetChild(content)?;
        } else {
            object
                .element
                .cast::<n::IContentControl>()?
                .SetContent(content)?;
        }
        object.children.replace(desired);
        Ok(())
    }
    fn caption(
        &self,
        node: &Rc<Node>,
        role: &str,
        text: &str,
        style: TextStyle,
    ) -> Result<n::UIElement> {
        let object = self.object(node, role, || n::TextBlock::new()?.cast())?;
        let label = object.element.cast::<n::ITextBlock>()?;
        if label.Text()? != text {
            label.SetText(text)?;
        }
        label.SetFontSize(
            f64::from(style.scale)
                * match style.role {
                    TextRole::Heading => 28.0,
                    TextRole::Subtitle => 20.0,
                    _ => 14.0,
                },
        )?;
        label.SetFontWeight(n::FontWeight {
            weight: if matches!(style.role, TextRole::Heading | TextRole::Subtitle) {
                600
            } else {
                400
            },
        })?;
        if let Some([r, g, b]) = style.foreground {
            let brush = n::SolidColorBrush::new()?;
            brush.SetColor(n::Color { a: 255, r, g, b })?;
            label.SetForeground(&brush)?;
        } else {
            label.SetForeground(object.foreground.as_ref())?;
        }
        Ok(object.element.clone())
    }
    pub(crate) fn root(&self) -> Result<n::UIElement> {
        let root = self.view.find(ROOT).map_err(sdk_error)?;
        self.objects
            .borrow()
            .get(&Self::key(&root, "wrapper"))
            .map(|o| o.element.clone())
            .ok_or_else(|| sdk_error("native root not rendered"))
    }
    pub(crate) fn apply(&self) -> Result<()> {
        if self.retired.get() {
            return Err(sdk_error("renderer retired"));
        }
        if self.applying.replace(true) {
            return Err(sdk_error("recursive native apply"));
        }
        struct Applying<'a>(&'a Cell<bool>);
        impl Drop for Applying<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _applying = Applying(&self.applying);
        self.used.borrow_mut().clear();
        let root = self.view.find(ROOT).map_err(sdk_error)?;
        self.render(&root, 0)?;
        let removed: Vec<_> = self
            .objects
            .borrow()
            .iter()
            .filter(|(k, _)| !self.used.borrow().contains(*k))
            .map(|(k, o)| (k.clone(), o.clone()))
            .collect();
        // Revoke removed generation callbacks before removing any native owner.
        for (_, object) in &removed {
            object.retire()?;
        }
        let objects: Vec<_> = self.objects.borrow().values().cloned().collect();
        let changed: Vec<_> = objects
            .into_iter()
            .filter(|o| {
                o.element.cast::<n::IPanel>().is_ok() && *o.children.borrow() != *o.desired.borrow()
            })
            .collect();
        // Clear ALL changed parents first. A keyed move cannot have two parents.
        for object in &changed {
            object.element.cast::<n::IPanel>()?.Children()?.Clear()?;
        }
        for object in changed {
            let children = object.desired.borrow().clone();
            let list = object.element.cast::<n::IPanel>()?.Children()?;
            for child in &children {
                list.Append(child)?;
            }
            object.children.replace(children);
        }
        for (key, _) in removed {
            self.objects.borrow_mut().remove(&key);
        }
        if self
            .last_focused_entry
            .get()
            .is_some_and(|token| !self.objects.borrow().values().any(|o| o.token == token))
        {
            self.last_focused_entry.set(None);
        }
        self.sync_accelerators()?;
        self.view.0.dirty.set(false);
        Ok(())
    }
    fn render(&self, node: &Rc<Node>, depth: usize) -> Result<n::UIElement> {
        if depth > super::MAX_DEPTH {
            return Err(sdk_error("native projection depth"));
        }
        let all_children: Vec<_> = node
            .children
            .borrow()
            .iter()
            .map(|id| self.view.find(id).map_err(sdk_error))
            .collect::<Result<_>>()?;
        // Window menus are chrome, not children of the content's layout.
        let children: Vec<_> = all_children
            .iter()
            .filter(|c| node.id != ROOT || !matches!(c.kind, Kind::Menu(_)))
            .cloned()
            .collect();
        let style = node.style.get();
        let control = match &node.kind {
            Kind::Panel(Layout::Stack {
                horizontal: false,
                gap,
            }) => self.vertical_stack(node, "control", &children, *gap, depth)?,
            Kind::Panel(Layout::Stack { horizontal, gap }) => {
                let o = self.panel(node, "control", false)?;
                let s = o.element.cast::<n::IStackPanel>()?;
                s.SetOrientation(if *horizontal {
                    n::Orientation::Horizontal
                } else {
                    n::Orientation::Vertical
                })?;
                s.SetSpacing(f64::from(*gap))?;
                self.child_list(
                    &o,
                    children
                        .iter()
                        .map(|c| self.render(c, depth + 1))
                        .collect::<Result<_>>()?,
                );
                o.element.clone()
            }
            Kind::Panel(Layout::Grid { columns, gap }) => {
                let o = self.panel(node, "control", true)?;
                let g = o.element.cast::<n::IGrid>()?;
                if *columns as usize > super::MAX_NODES {
                    return Err(sdk_error("native Grid column track budget"));
                }
                let mut rows = 1;
                let mut elements = Vec::new();
                for (index, c) in children.iter().enumerate() {
                    let p = c.style.get().placement;
                    let (r, col) = p.grid_position(index, *columns);
                    rows = rows.max(track_end(r, p.row_span)?);
                    let e = self.render(c, depth + 1)?;
                    let f = e.cast::<n::FrameworkElement>()?;
                    n::Grid::SetRow(&f, r)?;
                    n::Grid::SetColumn(&f, col)?;
                    n::Grid::SetRowSpan(&f, p.row_span)?;
                    n::Grid::SetColumnSpan(&f, p.column_span)?;
                    elements.push(e);
                }
                grid_definitions(
                    &g,
                    &vec![length(1.0, n::GridUnitType::Star); *columns as usize],
                    &vec![length(0.0, n::GridUnitType::Auto); rows as usize],
                )?;
                g.SetColumnSpacing(f64::from(*gap))?;
                g.SetRowSpacing(f64::from(*gap))?;
                self.child_list(&o, elements);
                o.element.clone()
            }
            Kind::Panel(Layout::Absolute) => {
                let o = self.object(node, "control", || n::Canvas::new()?.cast())?;
                let mut elements = Vec::new();
                for c in &children {
                    let e = self.render(c, depth + 1)?;
                    let p = c.style.get().placement;
                    n::Canvas::SetLeft(&e, f64::from(p.x))?;
                    n::Canvas::SetTop(&e, f64::from(p.y))?;
                    elements.push(e);
                }
                self.child_list(&o, elements);
                o.element.clone()
            }
            Kind::Panel(Layout::Flex { gap }) => {
                let o = self.object(node, "control", || super::owned_flex::panel(*gap)?.cast())?;
                self.child_list(
                    &o,
                    children
                        .iter()
                        .map(|c| self.render(c, depth + 1))
                        .collect::<Result<_>>()?,
                );
                o.element.clone()
            }
            Kind::Page(_) => self.vertical_stack(node, "control", &children, 0.0, depth)?,
            Kind::Scroll(horizontal) => {
                let o = self.object(node, "control", || n::ScrollViewer::new()?.cast())?;
                let scroll = o.element.cast::<n::IScrollViewer>()?;
                scroll.SetHorizontalScrollBarVisibility(if *horizontal {
                    n::ScrollBarVisibility::Auto
                } else {
                    n::ScrollBarVisibility::Disabled
                })?;
                scroll.SetVerticalScrollBarVisibility(if *horizontal {
                    n::ScrollBarVisibility::Disabled
                } else {
                    n::ScrollBarVisibility::Auto
                })?;
                let rows = self.panel(node, "rows", false)?;
                self.child_list(
                    &rows,
                    children
                        .iter()
                        .map(|c| self.render(c, depth + 1))
                        .collect::<Result<_>>()?,
                );
                let outer = self.panel(node, "scroll-body", false)?;
                let mut body = Vec::new();
                if let Some(vp) = node.viewport_info.get() {
                    let length = node.item_keys.borrow().len();
                    let range = vp.range(length).map_err(sdk_error)?;
                    let pager = self.panel(node, "pager", false)?;
                    pager
                        .element
                        .cast::<n::IStackPanel>()?
                        .SetOrientation(n::Orientation::Horizontal)?;
                    let prev = self.object(node, "previous", || n::Button::new()?.cast())?;
                    let next = self.object(node, "next", || n::Button::new()?.cast())?;
                    self.content(
                        &prev,
                        &self.caption(node, "prev-caption", "Previous", TextStyle::default())?,
                    )?;
                    self.content(
                        &next,
                        &self.caption(node, "next-caption", "Next", TextStyle::default())?,
                    )?;
                    prev.element
                        .cast::<n::IControl>()?
                        .SetIsEnabled(range.start > 0 && self.view.actionable(node))?;
                    next.element
                        .cast::<n::IControl>()?
                        .SetIsEnabled(range.end < length && self.view.actionable(node))?;
                    let position = self.caption(
                        node,
                        "position",
                        &format!(
                            "{}–{} / {length}",
                            if length == 0 { 0 } else { range.start + 1 },
                            range.end
                        ),
                        TextStyle::default(),
                    )?;
                    self.child_list(
                        &pager,
                        vec![prev.element.clone(), position, next.element.clone()],
                    );
                    body.push(pager.element.clone());
                }
                body.push(rows.element.clone());
                self.child_list(&outer, body);
                self.content(&o, &outer.element)?;
                o.element.clone()
            }
            Kind::Label => self.caption(
                node,
                "control",
                &node.text.borrow(),
                style.text.unwrap_or_default(),
            )?,
            Kind::Entry(binding) => {
                let o = self.object(node, "control", || n::TextBox::new()?.cast())?;
                let t = o.element.cast::<n::ITextBox>()?;
                if t.Text()? != *node.text.borrow() {
                    let text = node.text.borrow().clone();
                    t.SetText(&text)?;
                }
                t.SetPlaceholderText(&node.placeholder.borrow())?;
                t.SetIsReadOnly(!binding.is_writable())?;
                t.SetAcceptsReturn(false)?;
                o.element.clone()
            }
            Kind::Button(_) | Kind::TableCell(_) => {
                let o = self.object(node, "control", || n::Button::new()?.cast())?;
                self.content(
                    &o,
                    &self.caption(
                        node,
                        "caption",
                        &node.text.borrow(),
                        style.text.unwrap_or_default(),
                    )?,
                )?;
                if let Kind::TableCell(selection) = &node.kind {
                    let parent = node.parent.as_ref().and_then(|id| self.view.find(id).ok());
                    if let Some(parent) = parent
                        && let Kind::Table(t) = &parent.kind
                    {
                        self.accent(&o, t.selected.borrow().as_ref() == Some(selection))?;
                        o.element
                            .cast::<n::IControl>()?
                            .SetIsEnabled(t.selection.as_ref().is_some_and(|b| b.is_writable()))?;
                    }
                }
                o.element.clone()
            }
            Kind::Checkbox(_, checked) => {
                let o = self.object(node, "control", || n::CheckBox::new()?.cast())?;
                o.element
                    .cast::<n::IToggleButton>()?
                    .SetIsChecked(Some(checked.get()))?;
                self.content(
                    &o,
                    &self.caption(
                        node,
                        "caption",
                        &node.text.borrow(),
                        style.text.unwrap_or_default(),
                    )?,
                )?;
                o.element.clone()
            }
            Kind::Radio(_, value, selected) => {
                let o = self.object(node, "control", || n::RadioButton::new()?.cast())?;
                o.element
                    .cast::<n::IRadioButton>()?
                    .SetGroupName(&node.id)?;
                o.element
                    .cast::<n::IToggleButton>()?
                    .SetIsChecked(Some(*value == *selected.borrow()))?;
                self.content(
                    &o,
                    &self.caption(
                        node,
                        "caption",
                        &node.text.borrow(),
                        style.text.unwrap_or_default(),
                    )?,
                )?;
                o.element.clone()
            }
            Kind::Slider(_, range, value) => {
                let o = self.object(node, "control", || n::Slider::new()?.cast())?;
                let r = o.element.cast::<n::IRangeBase>()?;
                r.SetMinimum(range.min)?;
                r.SetMaximum(range.max)?;
                o.element
                    .cast::<n::ISlider>()?
                    .SetStepFrequency(range.step)?;
                r.SetValue(value.get())?;
                o.element.clone()
            }
            Kind::Combo(_, choices, selected) => {
                let o = self.object(node, "control", || n::ComboBox::new()?.cast())?;
                o.element.cast::<n::IComboBox>()?.SetIsEditable(false)?;
                if o.revision.get() != node.snapshot_revision.get()
                    || o.element.cast::<n::IItemsControl>()?.Items()?.Size()? == 0
                {
                    if o.revision.get() != node.snapshot_revision.get() {
                        o.revokers.take();
                        self.events(&o)?;
                    }
                    let list = o.element.cast::<n::IItemsControl>()?.Items()?;
                    list.Clear()?;
                    for choice in choices.borrow().iter() {
                        list.Append(&boxed(&choice.label)?)?;
                    }
                    o.revision.set(node.snapshot_revision.get());
                }
                let index = choices
                    .borrow()
                    .iter()
                    .position(|c| c.id == *selected.borrow())
                    .map_or(-1, |i| i as i32);
                o.element.cast::<n::ISelector>()?.SetSelectedIndex(index)?;
                o.element.clone()
            }
            Kind::Meter(value) => {
                let o = self.object(node, "control", || n::ProgressBar::new()?.cast())?;
                let r = o.element.cast::<n::IRangeBase>()?;
                r.SetMinimum(0.0)?;
                r.SetMaximum(1.0)?;
                r.SetValue(value.get())?;
                o.element
                    .cast::<n::IProgressBar>()?
                    .SetIsIndeterminate(false)?;
                o.element.clone()
            }
            Kind::Separator(vertical) => {
                let o = self.object(node, "control", || n::Border::new()?.cast())?;
                let b = n::SolidColorBrush::new()?;
                b.SetColor(n::Color {
                    a: 255,
                    r: 128,
                    g: 128,
                    b: 128,
                })?;
                o.element.cast::<n::IBorder>()?.SetBackground(&b)?;
                let f = o.element.cast::<n::IFrameworkElement>()?;
                f.SetWidth(if *vertical { 1.0 } else { f64::NAN })?;
                f.SetHeight(if *vertical { f64::NAN } else { 1.0 })?;
                o.element.clone()
            }
            Kind::Image(_, bytes, _) => {
                let o = self.object(node, "control", || n::Image::new()?.cast())?;
                o.element
                    .cast::<n::IImage>()?
                    .SetStretch(n::Stretch::Uniform)?;
                let bytes = bytes.borrow().as_ref().map(|b| b.as_bytes().to_vec());
                if *o.image.borrow() != bytes {
                    o.image.replace(bytes.clone());
                    self.image(&o, bytes)?;
                }
                o.element.clone()
            }
            Kind::Tabs(tabs) => {
                let outer = self.panel(node, "control", true)?;
                let header = self.panel(node, "headers", false)?;
                let pages = self.panel(node, "pages", true)?;
                let vertical =
                    matches!(tabs.position.get(), TabPosition::Left | TabPosition::Right);
                let reverse = matches!(
                    tabs.position.get(),
                    TabPosition::Bottom | TabPosition::Right
                );
                header
                    .element
                    .cast::<n::IStackPanel>()?
                    .SetOrientation(if vertical {
                        n::Orientation::Vertical
                    } else {
                        n::Orientation::Horizontal
                    })?;
                let mut headers = Vec::new();
                let mut contents = Vec::new();
                for page in &children {
                    let h = self.object(page, "header", || n::Button::new()?.cast())?;
                    self.content(
                        &h,
                        &self.caption(
                            page,
                            "header-caption",
                            &page.text.borrow(),
                            page.style.get().text.unwrap_or_default(),
                        )?,
                    )?;
                    h.element.cast::<n::IControl>()?.SetIsEnabled(
                        page.visible.get()
                            && page.enabled.get()
                            && tabs.selected.is_writable()
                            && self.view.actionable(node),
                    )?;
                    h.element.SetVisibility(if page.visible.get() {
                        n::Visibility::Visible
                    } else {
                        n::Visibility::Collapsed
                    })?;
                    self.accent(
                        &h,
                        matches!(&page.kind,Kind::Page(key) if *key==*tabs.value.borrow()),
                    )?;
                    headers.push(h.element.clone());
                    contents.push(self.render(page, depth + 1)?);
                }
                self.child_list(&header, headers);
                self.child_list(&pages, contents);
                let (h, p) = if reverse { (1, 0) } else { (0, 1) };
                if vertical {
                    grid_definitions(
                        &outer.element.cast()?,
                        &if reverse {
                            vec![
                                length(1.0, n::GridUnitType::Star),
                                length(0.0, n::GridUnitType::Auto),
                            ]
                        } else {
                            vec![
                                length(0.0, n::GridUnitType::Auto),
                                length(1.0, n::GridUnitType::Star),
                            ]
                        },
                        &[],
                    )?;
                    let hf = header.element.cast::<n::FrameworkElement>()?;
                    let pf = pages.element.cast::<n::FrameworkElement>()?;
                    n::Grid::SetColumn(&hf, h)?;
                    n::Grid::SetColumn(&pf, p)?;
                    n::Grid::SetRow(&hf, 0)?;
                    n::Grid::SetRow(&pf, 0)?;
                } else {
                    grid_definitions(
                        &outer.element.cast()?,
                        &[],
                        &if reverse {
                            vec![
                                length(1.0, n::GridUnitType::Star),
                                length(0.0, n::GridUnitType::Auto),
                            ]
                        } else {
                            vec![
                                length(0.0, n::GridUnitType::Auto),
                                length(1.0, n::GridUnitType::Star),
                            ]
                        },
                    )?;
                    let hf = header.element.cast::<n::FrameworkElement>()?;
                    let pf = pages.element.cast::<n::FrameworkElement>()?;
                    n::Grid::SetRow(&hf, h)?;
                    n::Grid::SetRow(&pf, p)?;
                    n::Grid::SetColumn(&hf, 0)?;
                    n::Grid::SetColumn(&pf, 0)?;
                }
                self.child_list(&outer, vec![header.element.clone(), pages.element.clone()]);
                outer.element.clone()
            }
            Kind::Dialog(..) => {
                let dialog = self.object(node, "control", || n::ContentDialog::new()?.cast())?;
                dialog
                    .element
                    .cast::<n::IContentDialog>()?
                    .SetTitle(&boxed(&node.text.borrow())?)?;
                let body = self.panel(node, "dialog-body", false)?;
                self.child_list(
                    &body,
                    children
                        .iter()
                        .map(|c| self.render(c, depth + 1))
                        .collect::<Result<_>>()?,
                );
                self.content(&dialog, &body.element)?;
                // Dialog is detached, never parented into the ordinary visual tree.
                let anchor = self.panel(node, "dialog-anchor", true)?;
                anchor.element.clone()
            }
            Kind::Table(table) => self.table(node, table, depth)?,
            Kind::Menu(menu) => self.menu(node, menu)?,
        };
        if let Ok(c) = control.cast::<n::IControl>() {
            let enabled=node.enabled.get() && self.view.actionable(node) && match &node.kind {
            Kind::Button(_)=>node.command_enabled.get(),Kind::Checkbox(b,_)=>b.is_writable(),Kind::Radio(b,..)=>b.is_writable(),Kind::Slider(b,..)=>b.is_writable(),Kind::Combo(b,..)=>b.is_writable(),
            Kind::TableCell(_)=>node.parent.as_ref().and_then(|id|self.view.find(id).ok()).is_some_and(|p|matches!(&p.kind,Kind::Table(t) if t.selection.as_ref().is_some_and(|b|b.is_writable()))),_=>true};
            c.SetIsEnabled(enabled)?;
        }
        if matches!(node.kind, Kind::Menu(_)) {
            // MenuBar owns these directly; a Border wrapper would give the
            // same item a second native parent when it is inserted in Items.
            return Ok(control);
        }
        let padding = self.object(node, "padding", || n::Border::new()?.cast())?;
        padding
            .element
            .cast::<n::IBorder>()?
            .SetPadding(thickness(style.padding))?;
        self.content(&padding, &control)?;
        let content = if node.id == ROOT {
            let shell = self.panel(node, "window-shell", true)?;
            let bar = self.object(node, "menubar", || n::MenuBar::new()?.cast())?;
            let mut menus = Vec::new();
            let mut visible_menu = false;
            for child in &all_children {
                if matches!(child.kind, Kind::Menu(_)) {
                    // Render appearance/guards on the MenuBarItem itself. The
                    // item, never its ordinary wrapper, belongs to Items.
                    self.render(child, depth + 1)?;
                    visible_menu |= child.visible.get();
                    menus.push(
                        self.find(&child.id, "control")?
                            .element
                            .cast::<n::MenuBarItem>()?,
                    );
                }
            }
            sync_items(&bar.element.cast::<n::IMenuBar>()?.Items()?, &menus)?;
            bar.element.SetVisibility(if visible_menu {
                n::Visibility::Visible
            } else {
                n::Visibility::Collapsed
            })?;
            grid_definitions(
                &shell.element.cast()?,
                &[],
                &[
                    length(0.0, n::GridUnitType::Auto),
                    length(1.0, n::GridUnitType::Star),
                ],
            )?;
            n::Grid::SetRow(&padding.element.cast::<n::FrameworkElement>()?, 1)?;
            self.child_list(&shell, vec![bar.element.clone(), padding.element.clone()]);
            shell.element.clone()
        } else {
            padding.element.clone()
        };
        let wrapper = self.panel(node, "wrapper", true)?;
        let f = wrapper.element.cast::<n::IFrameworkElement>()?;
        f.SetWidth(style.placement.width.map_or(f64::NAN, f64::from))?;
        f.SetHeight(style.placement.height.map_or(f64::NAN, f64::from))?;
        f.SetMargin(thickness(style.margin))?;
        f.SetHorizontalAlignment(
            match style.horizontal_alignment.unwrap_or(Alignment::Stretch) {
                Alignment::Start => n::HorizontalAlignment::Left,
                Alignment::Center => n::HorizontalAlignment::Center,
                Alignment::End => n::HorizontalAlignment::Right,
                Alignment::Stretch => n::HorizontalAlignment::Stretch,
            },
        )?;
        f.SetVerticalAlignment(
            match style.vertical_alignment.unwrap_or(Alignment::Stretch) {
                Alignment::Start => n::VerticalAlignment::Top,
                Alignment::Center => n::VerticalAlignment::Center,
                Alignment::End => n::VerticalAlignment::Bottom,
                Alignment::Stretch => n::VerticalAlignment::Stretch,
            },
        )?;
        wrapper.element.SetVisibility(
            if node.visible.get() && self.view.actionable_page(node) {
                n::Visibility::Visible
            } else {
                n::Visibility::Collapsed
            },
        )?;
        self.child_list(&wrapper, vec![content]);
        Ok(wrapper.element.clone())
    }
    // Star tracks receive the finite remainder after natural-height siblings.
    // Under an unbounded ScrollViewer measure, XAML treats Star as Auto, so
    // nested scrolling pages/dialogs retain natural content measurement.
    fn vertical_stack(
        &self,
        node: &Rc<Node>,
        role: &str,
        children: &[Rc<Node>],
        gap: f32,
        depth: usize,
    ) -> Result<n::UIElement> {
        let o = self.panel(node, role, true)?;
        let grid = o.element.cast::<n::IGrid>()?;
        let mut elements = Vec::new();
        let mut tracks = Vec::new();
        for child in children {
            let e = self.render(child, depth + 1)?;
            if e.Visibility()? == n::Visibility::Collapsed || matches!(child.kind, Kind::Dialog(..))
            {
                n::Grid::SetRow(&e.cast::<n::FrameworkElement>()?, 0)?;
                elements.push(e);
                continue;
            }
            n::Grid::SetRow(&e.cast::<n::FrameworkElement>()?, tracks.len() as i32)?;
            tracks.push(if fills_height(&self.view, child, depth + 1)? {
                length(1.0, n::GridUnitType::Star)
            } else {
                length(0.0, n::GridUnitType::Auto)
            });
            elements.push(e);
        }
        grid_definitions(&grid, &[], &tracks)?;
        grid.SetRowSpacing(f64::from(gap))?;
        self.child_list(&o, elements);
        Ok(o.element.clone())
    }
    fn table(
        &self,
        node: &Rc<Node>,
        table: &super::TableState,
        depth: usize,
    ) -> Result<n::UIElement> {
        let data = table
            .data
            .borrow()
            .clone()
            .ok_or_else(|| sdk_error("table snapshot missing"))?;
        let outer = self.object(node, "control", || n::ScrollViewer::new()?.cast())?;
        outer
            .element
            .cast::<n::IScrollViewer>()?
            .SetVerticalScrollBarVisibility(n::ScrollBarVisibility::Auto)?;
        let rows = self.panel(node, "table-rows", false)?;
        rows.element
            .cast::<n::IStackPanel>()?
            .SetSpacing(f64::from(data.row_gap))?;
        let columns: Vec<_> = data
            .columns
            .iter()
            .map(|c| match c.width {
                Some(w) => length(f64::from(w), n::GridUnitType::Pixel),
                None if c.stretch => length(1.0, n::GridUnitType::Star),
                None => length(0.0, n::GridUnitType::Auto),
            })
            .collect();
        let header = self.panel(node, "table-headers", true)?;
        grid_definitions(&header.element.cast()?, &columns, &[])?;
        header
            .element
            .cast::<n::IGrid>()?
            .SetColumnSpacing(f64::from(data.column_gap))?;
        let mut headers = Vec::new();
        for (i, c) in data.columns.iter().enumerate() {
            let e = self.caption(node, &format!("column:{i}"), &c.title, c.text)?;
            n::Grid::SetColumn(&e.cast::<n::FrameworkElement>()?, i as i32)?;
            headers.push(e);
        }
        self.child_list(&header, headers);
        let mut native_rows = vec![header.element.clone()];
        if data.rows.is_empty() {
            native_rows.push(self.caption(
                node,
                "empty",
                &data.empty_text,
                TextStyle::default(),
            )?);
        }
        for row in data.rows.iter() {
            let grid = self.panel(node, &format!("row:{}", row.key), true)?;
            grid_definitions(&grid.element.cast()?, &columns, &[])?;
            grid.element
                .cast::<n::IGrid>()?
                .SetColumnSpacing(f64::from(data.column_gap))?;
            let mut cells = Vec::new();
            for (i, _) in row.cells.iter().enumerate() {
                let id = format!("{}/{}/{i}", node.id, row.key);
                let child = table.cells.borrow().get(&id).cloned();
                if let Some(child) = child {
                    let element = if table.selection.is_some() {
                        self.render(&child, depth + 1)?
                    } else {
                        self.caption(
                            &child,
                            "readonly-caption",
                            &child.text.borrow(),
                            child.style.get().text.unwrap_or_default(),
                        )?
                    };
                    let label = self
                        .objects
                        .borrow()
                        .get(&Self::key(
                            &child,
                            if table.selection.is_some() {
                                "caption"
                            } else {
                                "readonly-caption"
                            },
                        ))
                        .cloned()
                        .ok_or_else(|| sdk_error("table caption missing"))?;
                    label.element.cast::<n::ITextBlock>()?.SetTextWrapping(
                        if data.columns[i].overflow == TextOverflow::Wrap {
                            n::TextWrapping::Wrap
                        } else {
                            n::TextWrapping::NoWrap
                        },
                    )?;
                    label
                        .element
                        .cast::<n::ITextBlock>()?
                        .SetTextTrimming(n::TextTrimming::Clip)?;
                    n::Grid::SetColumn(&element.cast::<n::FrameworkElement>()?, i as i32)?;
                    cells.push(element);
                }
            }
            self.child_list(&grid, cells);
            let inset = self.object(node, &format!("row-inset:{}", row.key), || {
                n::Border::new()?.cast()
            })?;
            inset
                .element
                .cast::<n::IBorder>()?
                .SetPadding(n::Thickness {
                    left: f64::from(data.row_inset),
                    right: f64::from(data.row_inset),
                    top: f64::from(data.row_inset),
                    bottom: f64::from(data.row_inset),
                })?;
            self.content(&inset, &grid.element)?;
            native_rows.push(inset.element.clone());
        }
        self.child_list(&rows, native_rows);
        self.content(&outer, &rows.element)?;
        Ok(outer.element.clone())
    }
    fn menu(&self, node: &Rc<Node>, menu: &super::MenuState) -> Result<n::UIElement> {
        let item = self.object(node, "control", || n::MenuBarItem::new()?.cast())?;
        item.element
            .cast::<n::IMenuBarItem>()?
            .SetTitle(&node.text.borrow())?;
        item.element
            .cast::<n::IControl>()?
            .SetIsEnabled(self.view.actionable(node))?;
        let native = menu.native.borrow().clone();
        let built = self.menu_items(node, &native, "menu", 0)?;
        sync_items(&item.element.cast::<n::IMenuBarItem>()?.Items()?, &built)?;
        item.element.SetVisibility(if node.visible.get() {
            n::Visibility::Visible
        } else {
            n::Visibility::Collapsed
        })?;
        Ok(item.element.clone())
    }
    fn menu_items(
        &self,
        node: &Rc<Node>,
        items: &[windows_reactor::MenuItem],
        path: &str,
        depth: usize,
    ) -> Result<Vec<n::MenuFlyoutItemBase>> {
        if depth > super::MAX_DEPTH {
            return Err(sdk_error("native menu depth"));
        }
        let mut result = Vec::new();
        for item in items {
            match item {
                windows_reactor::MenuItem::Separator { key } => {
                    let o = self.object(node, &format!("{path}/separator:{key:?}"), || {
                        n::MenuFlyoutSeparator::new()?.cast()
                    })?;
                    result.push(o.element.cast()?);
                }
                windows_reactor::MenuItem::Submenu { key, label, items } => {
                    let role = format!("{path}/submenu:{key:?}");
                    let o = self.object(node, &role, || n::MenuFlyoutSubItem::new()?.cast())?;
                    let sub = o.element.cast::<n::IMenuFlyoutSubItem>()?;
                    sub.SetText(label)?;
                    let metadata = self.menu_metadata(node, key)?;
                    o.element
                        .cast::<n::IControl>()?
                        .SetIsEnabled(metadata.1.enabled && self.view.actionable(node))?;
                    let children = self.menu_items(node, items, &role, depth + 1)?;
                    sync_items(&sub.Items()?, &children)?;
                    result.push(o.element.cast()?);
                }
                windows_reactor::MenuItem::Item {
                    key,
                    label,
                    enabled,
                } => {
                    let o = self.object(node, &format!("{path}/item:{key:?}"), || {
                        n::MenuFlyoutItem::new()?.cast()
                    })?;
                    let item = o.element.cast::<n::IMenuFlyoutItem>()?;
                    let (id, metadata) = self.menu_metadata(node, key)?;
                    o.menu_id.replace(Some(id));
                    item.SetText(label)?;
                    item.SetKeyboardAcceleratorTextOverride(&owned_menu::shortcut_caption(
                        &metadata,
                    ))?;
                    o.element
                        .cast::<n::IControl>()?
                        .SetIsEnabled(*enabled && self.view.actionable(node))?;
                    if o.menu_event.borrow().is_none()
                        || o.revision.get() != node.snapshot_revision.get()
                    {
                        o.menu_event.take();
                        let generation = self.generation;
                        let epoch = self.menu_epoch;
                        let token = o.token;
                        let revision = node.snapshot_revision.get();
                        o.menu_event.replace(Some(item.Click(move |_, _| {
                            callback(|| {
                                with_renderer(generation, |r| {
                                    if r.menu_epoch != epoch {
                                        return Ok(());
                                    }
                                    r.invoke_menu(token, revision).map(|_| ())
                                })
                            })
                        })?));
                        o.revision.set(revision);
                    }
                    result.push(o.element.cast()?);
                }
            }
        }
        Ok(result)
    }
    fn events(&self, o: &Object) -> Result<()> {
        let generation = self.generation;
        let token = o.token;
        let revision = o.node.snapshot_revision.get();
        let mut revokers = Vec::new();
        if o.role == "header"
            || o.role == "previous"
            || o.role == "next"
            || matches!(o.node.kind, Kind::Button(_) | Kind::TableCell(_))
        {
            revokers.push(o.element.cast::<n::IButtonBase>()?.Click(move |_, _| {
                callback(|| with_renderer(generation, |r| r.event(token, revision, Event::Click)))
            })?);
        } else {
            match &o.node.kind {
                Kind::Entry(_) => {
                    let epoch = self.menu_epoch;
                    revokers.push(o.element.GotFocus(move |_, _| {
                        callback(|| {
                            with_renderer(generation, |r| {
                                if r.menu_epoch == epoch {
                                    r.focus_entry(token);
                                } else {
                                    eprintln!(
                                        "NATIVE_GOT_FOCUS_REJECT generation={generation} token={token} event-epoch={epoch} renderer-epoch={}",
                                        r.menu_epoch
                                    );
                                }
                                Ok(())
                            })
                        })
                    })?);
                    revokers.push(o.element.cast::<n::ITextBox>()?.TextChanged(move |_, _| {
                        callback(|| {
                            with_renderer(generation, |r| r.event(token, revision, Event::Text))
                        })
                    })?)
                }
                Kind::Checkbox(..) | Kind::Radio(..) => {
                    let t = o.element.cast::<n::IToggleButton>()?;
                    revokers.push(t.Checked(move |_, _| {
                        callback(|| {
                            with_renderer(generation, |r| r.event(token, revision, Event::Check))
                        })
                    })?);
                    revokers.push(t.Unchecked(move |_, _| {
                        callback(|| {
                            with_renderer(generation, |r| r.event(token, revision, Event::Check))
                        })
                    })?);
                }
                Kind::Slider(..) => {
                    revokers.push(o.element.cast::<n::IRangeBase>()?.ValueChanged(
                        move |_, _| {
                            callback(|| {
                                with_renderer(generation, |r| {
                                    r.event(token, revision, Event::Value)
                                })
                            })
                        },
                    )?)
                }
                Kind::Combo(..) => {
                    revokers.push(o.element.cast::<n::ISelector>()?.SelectionChanged(
                        move |_, _| {
                            callback(|| {
                                with_renderer(generation, |r| {
                                    r.event(token, revision, Event::Selection)
                                })
                            })
                        },
                    )?)
                }
                Kind::Dialog(..) => revokers.push(o.element.cast::<n::IContentDialog>()?.Closed(
                    move |_, args| {
                        callback(|| {
                            with_renderer(generation, |r| {
                                r.event(token, revision, Event::Dialog(args.ok()?.Result()?))
                            })
                        })
                    },
                )?),
                Kind::Image(_, _, Some(_)) => {
                    revokers.push(o.element.PointerMoved(move |_, args| {
                        callback(|| with_renderer(generation, |r| r.pointer(token, args, false)))
                    })?);
                    revokers.push(o.element.PointerExited(move |_, args| {
                        callback(|| with_renderer(generation, |r| r.pointer(token, args, true)))
                    })?);
                }
                _ => (),
            }
        }
        o.revokers.replace(revokers);
        Ok(())
    }
    fn event(&self, token: u64, revision: u64, event: Event) -> Result<()> {
        if self.retired.get() || self.applying.get() {
            return Ok(());
        }
        let object = self
            .objects
            .borrow()
            .values()
            .find(|o| o.token == token)
            .cloned();
        let Some(o) = object else {
            return Ok(());
        };
        if !self.view.live(&o.node) {
            return Ok(());
        }
        let node = &o.node;
        if matches!(event, Event::Selection) && node.snapshot_revision.get() != revision {
            return Ok(());
        }
        o.changes.set(
            o.changes
                .get()
                .checked_add(1)
                .ok_or_else(|| sdk_error("native event count exhausted"))?,
        );
        match event {
            Event::Click => {
                let cb = match o.role.as_str() {
                    "previous" => node.previous_page.borrow().clone(),
                    "next" => node.next_page.borrow().clone(),
                    _ => node.clicked.borrow().clone(),
                };
                if let Some(cb) = cb {
                    let _ = cb.call(());
                }
            }
            Event::Text => {
                let text = o.element.cast::<n::ITextBox>()?.Text()?;
                let cb = node.text_changed.borrow().clone();
                if let Some(cb) = cb {
                    let _ = cb.call(text.clone());
                }
                eprintln!(
                    "NATIVE_TEXT_CHANGED generation={} token={} text={text:?}",
                    self.generation, token
                );
            }
            Event::Check => {
                let value = o.element.cast::<n::IToggleButton>()?.IsChecked()?;
                let cb = node.checked_changed.borrow().clone();
                if let Some(cb) = cb {
                    let _ = cb.call(value);
                }
            }
            Event::Value => {
                let value = o.element.cast::<n::IRangeBase>()?.Value()?;
                let cb = node.value_changed.borrow().clone();
                if let Some(cb) = cb {
                    let _ = cb.call(value);
                }
            }
            Event::Selection => {
                if o.revision.get() != node.snapshot_revision.get() {
                    return Ok(());
                }
                let value = o.element.cast::<n::ISelector>()?.SelectedIndex()?;
                let cb = node.selection_changed.borrow().clone();
                if let Some(cb) = cb {
                    let _ = cb.call(usize::try_from(value).ok());
                }
            }
            Event::Dialog(result) => {
                o.showing.set(false);
                o.operation.take();
                for abort in o.abort.take() {
                    abort.abort();
                }
                let cb = node.dialog_closed.borrow().clone();
                if let Some(cb) = cb {
                    let result = match result {
                        n::ContentDialogResult::Primary => {
                            windows_reactor::ContentDialogResult::Primary
                        }
                        n::ContentDialogResult::Secondary => {
                            windows_reactor::ContentDialogResult::Secondary
                        }
                        _ => windows_reactor::ContentDialogResult::None,
                    };
                    let _ = cb.call(result);
                }
            }
        }
        Ok(())
    }
    fn accent(&self, o: &Object, selected: bool) -> Result<()> {
        let control = o.element.cast::<n::IControl>()?;
        if selected {
            let brush = n::SolidColorBrush::new()?;
            brush.SetColor(n::Color {
                a: 255,
                r: 40,
                g: 100,
                b: 190,
            })?;
            control.SetBackground(&brush)?;
        } else {
            control.SetBackground(o.background.as_ref())?;
        }
        Ok(())
    }
    fn pointer(
        &self,
        token: u64,
        args: windows_core::Ref<n::PointerRoutedEventArgs>,
        exited: bool,
    ) -> Result<()> {
        if self.applying.get() || self.retired.get() {
            return Ok(());
        }
        let o = self
            .objects
            .borrow()
            .values()
            .find(|o| o.token == token)
            .cloned();
        let Some(o) = o else {
            return Ok(());
        };
        if !self.view.live(&o.node) {
            return Ok(());
        }
        let point = args.ok()?.GetCurrentPoint(&o.element)?;
        let p = point.Position()?;
        let w = args.ok()?.GetCurrentPoint(&self.root()?)?.Position()?;
        let properties = point.Properties()?;
        let info = windows_reactor::PointerEventInfo {
            x: f64::from(p.x),
            y: f64::from(p.y),
            window_x: f64::from(w.x),
            window_y: f64::from(w.y),
            capture_succeeded: false,
            is_left_button_pressed: properties.IsLeftButtonPressed()?,
            is_right_button_pressed: properties.IsRightButtonPressed()?,
            is_middle_button_pressed: properties.IsMiddleButtonPressed()?,
        };
        let cb = if exited {
            o.node.pointer_exited.borrow().clone()
        } else {
            o.node.pointer_moved.borrow().clone()
        };
        if let Some(cb) = cb {
            let _ = cb.call(info);
        }
        Ok(())
    }
    fn image(&self, o: &Rc<Object>, bytes: Option<Vec<u8>>) -> Result<()> {
        for abort in o.abort.take() {
            abort.abort();
        }
        let revision = o
            .image_revision
            .get()
            .checked_add(1)
            .ok_or_else(|| sdk_error("image revision exhausted"))?;
        o.image_revision.set(revision);
        let image = o.element.cast::<n::IImage>()?;
        let Some(bytes) = bytes else {
            image.SetSource(None::<&n::ImageSource>)?;
            return Ok(());
        };
        let bitmap = n::BitmapImage::new()?;
        let stream = n::InMemoryRandomAccessStream::new()?;
        let writer = n::DataWriter::CreateDataWriter(&stream.GetOutputStreamAt(0)?)?;
        writer.WriteBytes(&bytes)?;
        let generation = self.generation;
        let token = o.token;
        let (abort, registration) = AbortHandle::new_pair();
        o.abort.borrow_mut().push(abort);
        self.bridge.spawn(
            async move {
                let work = async move {
                    writer.StoreAsync()?.await?;
                    stream.Seek(0)?;
                    bitmap
                        .cast::<n::IBitmapSource>()?
                        .SetSourceAsync(&stream)?
                        .await?;
                    with_renderer(generation, |r| {
                        let o = r
                            .objects
                            .borrow()
                            .values()
                            .find(|o| o.token == token)
                            .cloned();
                        if let Some(o) = o
                            && r.view.live(&o.node)
                            && o.image_revision.get() == revision
                        {
                            image.SetSource(&bitmap)?;
                        }
                        Ok(())
                    })
                };
                if let Ok(Err(error)) = Abortable::new(work, registration).await {
                    callback(|| Err(error));
                }
            }
            .boxed_local(),
        );
        Ok(())
    }
    pub(crate) fn dialogs(&self) -> Result<()> {
        if self.retired.get() {
            return Ok(());
        }
        let root = self.root()?;
        if !root.cast::<n::IFrameworkElement>()?.IsLoaded()? {
            return Ok(());
        }
        let objects: Vec<_> = self
            .objects
            .borrow()
            .values()
            .filter(|o| o.role == "control" && matches!(o.node.kind, Kind::Dialog(..)))
            .cloned()
            .collect();
        for o in objects {
            let Kind::Dialog(_, open) = &o.node.kind else {
                continue;
            };
            let dialog = o.element.cast::<n::IContentDialog>()?;
            let wanted = open.get() && self.view.actionable(&o.node);
            if wanted && !o.showing.get() {
                o.element.SetXamlRoot(&root.XamlRoot()?)?;
                let operation = dialog.ShowAsync()?;
                o.showing.set(true);
                o.operation.replace(Some(operation.clone()));
                let (abort, registration) = AbortHandle::new_pair();
                o.abort.borrow_mut().push(abort);
                self.bridge.spawn(
                    async move {
                        if let Ok(Err(error)) =
                            Abortable::new(async move { operation.await }, registration).await
                        {
                            callback(|| Err(error));
                        }
                    }
                    .boxed_local(),
                );
            } else if !wanted && o.showing.get() {
                dialog.Hide()?;
            }
        }
        Ok(())
    }
    pub(crate) fn hide_dialogs(&self) -> Result<()> {
        let objects: Vec<_> = self.objects.borrow().values().cloned().collect();
        for o in objects {
            if o.showing.get() {
                o.element.cast::<n::IContentDialog>()?.Hide()?;
            }
        }
        Ok(())
    }
    pub(crate) fn quiesce(&self) {
        self.view.0.native_edit.take();
        self.last_focused_entry.set(None);
        let accelerators = self.accelerators.take();
        for accelerator in accelerators.into_values() {
            // Quiesce cannot return errors; callback preserves the driver's fault path.
            callback(|| accelerator.native.SetIsEnabled(false));
        }
        let root = self
            .objects
            .borrow()
            .values()
            .find(|o| o.role == "wrapper" && o.node.id == ROOT)
            .cloned();
        if let Some(root) = root {
            callback(|| root.element.KeyboardAccelerators()?.Clear());
        }
        self.last_revoked_accelerator.take();
        let objects: Vec<_> = self.objects.borrow().values().cloned().collect();
        for o in objects {
            o.revokers.take();
            o.menu_event.take();
            for abort in o.abort.take() {
                abort.abort();
            }
        }
    }
    pub(crate) fn retire(&self) -> Result<()> {
        if self.retired.replace(true) {
            return Ok(());
        }
        self.quiesce();
        let objects = self.objects.take();
        for o in objects.values() {
            o.retire()?;
        }
        Ok(())
    }
    fn find(&self, id: &str, role: &str) -> Result<Rc<Object>> {
        let node = self.view.find(id).map_err(sdk_error)?;
        self.objects
            .borrow()
            .get(&Self::key(&node, role))
            .cloned()
            .ok_or_else(|| sdk_error(format!("native node missing: {id}/{role}")))
    }
    pub(crate) fn diagnostic_text_probe(
        &self,
        entry: &str,
        label: &str,
        set_text: Option<&str>,
        hwnd: usize,
    ) -> Result<Option<crate::smoke::NativeTextProbe>> {
        if self.retired.get() || self.applying.get() || self.view.is_closed() {
            return Err(sdk_error("diagnostic renderer unavailable"));
        }
        let root = self.find(ROOT, "wrapper")?;
        let input = self.find(entry, "control")?;
        let output = self.find(label, "control")?;
        if !matches!(input.node.kind, Kind::Entry(_)) || !matches!(output.node.kind, Kind::Label) {
            return Err(sdk_error("diagnostic requires TextBox and Label IDs"));
        }
        for o in [&root, &input, &output] {
            if !self.view.live(&o.node) {
                return Err(sdk_error("diagnostic native node identity retired"));
            }
            if !o.loaded.get() || !o.element.cast::<n::IFrameworkElement>()?.IsLoaded()? {
                return Ok(None);
            }
        }
        let xaml = root.element.XamlRoot()?;
        if input.element.XamlRoot()? != xaml || output.element.XamlRoot()? != xaml {
            return Err(sdk_error("diagnostic controls have foreign XamlRoot"));
        }
        let textbox = input.element.cast::<n::ITextBox>()?;
        if let Some(value) = set_text {
            if textbox.IsReadOnly()? || !self.view.actionable(&input.node) {
                return Err(sdk_error("diagnostic TextBox is not writable/live"));
            }
            textbox.SetText(value)?;
        }
        if self.retired.get() || !self.view.live(&input.node) || !self.view.live(&output.node) {
            return Err(sdk_error("diagnostic nodes retired during native input"));
        }
        Ok(Some(crate::smoke::NativeTextProbe {
            generation: self.generation,
            hwnd,
            entry_token: input.token,
            label_token: output.token,
            entry_text: textbox.Text()?,
            label_text: output.element.cast::<n::ITextBlock>()?.Text()?,
            text_events: input.changes.get(),
        }))
    }
    pub(crate) fn set_text(&self, id: &str, value: &str) -> Result<()> {
        self.find(id, "control")?
            .element
            .cast::<n::ITextBox>()?
            .SetText(value)
    }
    pub(crate) fn node_identity(&self, id: &str) -> Result<(u64, usize)> {
        let node = self.view.find(id).map_err(sdk_error)?;
        let o = self.find(
            id,
            if matches!(node.kind, Kind::Menu(_)) {
                "control"
            } else {
                "wrapper"
            },
        )?;
        Ok((o.token, o.element.as_raw() as usize))
    }
    pub(crate) fn scroll_fixture(&self, end: bool) -> Result<()> {
        let scroll = self
            .find("scroll", "control")?
            .element
            .cast::<n::IScrollViewer>()?;
        let target = if end { scroll.ScrollableHeight()? } else { 0.0 };
        if !scroll.ChangeViewWithOptionalAnimation(None, Some(target), None, true)?
            && (scroll.VerticalOffset()? - target).abs() > 1.0
        {
            return Err(sdk_error("native ScrollViewer rejected ChangeView"));
        }
        Ok(())
    }
    pub(crate) fn window_layout_assertions(&self, end: bool) -> Result<f64> {
        self.root()?.UpdateLayout()?;
        let shell = self.find(ROOT, "window-shell")?.element.clone();
        let bar = self.find(ROOT, "menubar")?.element.clone();
        let content = self.find(ROOT, "padding")?.element.clone();
        let near = |a: f32, b: f32| (a - b).abs() < 1.0;
        let shell_size = shell.RenderSize()?;
        let bar_size = bar.RenderSize()?;
        let content_size = content.RenderSize()?;
        if bar_size.height <= 0.0
            || !near(bar.ActualOffset()?.y, 0.0)
            || !near(content.ActualOffset()?.y, bar_size.height)
            || !near(content_size.height + bar_size.height, shell_size.height)
        {
            return Err(sdk_error(format!(
                "native menu/content boundary shell={shell_size:?} bar={bar_size:?} content={content_size:?}"
            )));
        }
        let items = bar.cast::<n::IMenuBar>()?.Items()?;
        let menu_nodes: Vec<_> = self
            .view
            .find(ROOT)
            .map_err(sdk_error)?
            .children
            .borrow()
            .iter()
            .map(|id| self.view.find(id).map_err(sdk_error))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|node| matches!(node.kind, Kind::Menu(_)))
            .collect();
        if items.Size()? as usize != menu_nodes.len() || menu_nodes.len() != 3 {
            return Err(sdk_error("native common MenuBar item count"));
        }
        let mut previous_right = 0.0;
        let mut first_y = None;
        for (index, node) in menu_nodes.iter().enumerate() {
            let element = self.find(&node.id, "control")?.element.clone();
            if items.GetAt(index as u32)? != element.cast::<n::MenuBarItem>()?
                || element.cast::<n::IMenuBarItem>()?.Title()? != *node.text.borrow()
                || (element.Visibility()? == n::Visibility::Visible) != node.visible.get()
            {
                return Err(sdk_error("native common MenuBar order/title/visibility"));
            }
            if !node.visible.get() {
                continue;
            }
            let offset = element.ActualOffset()?;
            let size = element.RenderSize()?;
            if size.width <= 0.0
                || offset.x + 1.0 < previous_right
                || first_y.is_some_and(|y| !near(y, offset.y))
            {
                return Err(sdk_error("native MenuBar items are not horizontal"));
            }
            first_y = Some(offset.y);
            previous_right = offset.x + size.width;
        }
        let header = self.find("pinned-header", "wrapper")?.element.clone();
        let footer = self.find("pinned-footer", "wrapper")?.element.clone();
        let viewport = self.find("scroll", "wrapper")?.element.clone();
        let stack = self.find("window-body", "control")?.element.clone();
        let scroll = self
            .find("scroll", "control")?
            .element
            .cast::<n::IScrollViewer>()?;
        let viewport_height = scroll.ViewportHeight()?;
        let scrollable = scroll.ScrollableHeight()?;
        let offset = scroll.VerticalOffset()?;
        if viewport_height <= 0.0
            || !viewport_height.is_finite()
            || scrollable <= 0.0
            || scroll.VerticalScrollBarVisibility()? != n::ScrollBarVisibility::Auto
            || !near(header.ActualOffset()?.y, 0.0)
            || !near(
                viewport.ActualOffset()?.y,
                header.RenderSize()?.height + 5.0,
            )
            || !near(
                footer.ActualOffset()?.y + footer.RenderSize()?.height,
                stack.RenderSize()?.height,
            )
            || !near(
                footer.ActualOffset()?.y,
                viewport.ActualOffset()?.y + viewport.RenderSize()?.height + 5.0,
            )
            || (offset - if end { scrollable } else { 0.0 }).abs() > 1.0
        {
            return Err(sdk_error(format!(
                "native bounded scrolling/pinned footer viewport={viewport_height} scrollable={scrollable} offset={offset} end={end}"
            )));
        }
        println!(
            "NATIVE_WINDOW_LAYOUT_PASS generation={} viewport={viewport_height} scrollable={scrollable} offset={offset} end={end} common-menubar-horizontal=true chrome-footer-pinned=true native-readback=true no-physical-input-claim",
            self.generation
        );
        Ok(viewport_height)
    }
    pub(crate) fn hold_entry(&self, id: &str) -> Result<u64> {
        let o = self.find(id, "control")?;
        self.assert_entry_focus(id)?;
        self.stale_entry.replace(Some((o.token, o.element.cast()?)));
        Ok(o.token)
    }
    pub(crate) fn removed_token(&self, token: u64) -> Result<()> {
        if self.objects.borrow().values().any(|o| o.token == token) {
            return Err(sdk_error("removed native token retained"));
        }
        let stale = self
            .stale_entry
            .borrow()
            .clone()
            .ok_or_else(|| sdk_error("stale native fixture absent"))?;
        if stale.0 != token {
            return Err(sdk_error("stale native fixture token"));
        }
        stale.1.SetText("obsolete native event")?;
        self.event(token, 0, Event::Text)?;
        let saved = self.last_focused_entry.replace(Some(token));
        let rejected = self.view.activate_menu("edit-select").is_err();
        self.last_focused_entry.set(saved);
        if !rejected {
            return Err(sdk_error(
                "removed native editor token routed to a replacement",
            ));
        }
        println!(
            "NATIVE_REMOVED_TOKEN_PASS generation={} old-token={token} old-TextBox.SetText=true revoked-callback-no-target=true",
            self.generation
        );
        Ok(())
    }
    pub(crate) fn selectors(&self) -> Result<()> {
        self.find("check", "control")?
            .element
            .cast::<n::IToggleButton>()?
            .SetIsChecked(Some(true))?;
        self.find("slider", "control")?
            .element
            .cast::<n::IRangeBase>()?
            .SetValue(0.75)?;
        self.find("combo", "control")?
            .element
            .cast::<n::ISelector>()?
            .SetSelectedIndex(1)?;
        Ok(())
    }
    pub(crate) fn dialog_fact(&self, open: bool) -> Result<()> {
        let o = self.find("dialog", "control")?;
        if o.showing.get() != open {
            return Err(sdk_error("native ContentDialog showing mismatch"));
        }
        if open {
            if o.element.XamlRoot()? != self.root()?.XamlRoot()? || o.operation.borrow().is_none() {
                return Err(sdk_error("dialog XamlRoot/async operation missing"));
            }
        } else if o.changes.get() == 0 {
            return Err(sdk_error("real ContentDialog Closed event missing"));
        }
        println!(
            "NATIVE_DIALOG_PASS generation={} open={open} shared-XamlRoot=true closed-events={}",
            self.generation,
            o.changes.get()
        );
        Ok(())
    }
    pub(crate) fn assertions(&self) -> Result<()> {
        let root = self.root()?;
        if !root.cast::<n::IFrameworkElement>()?.IsLoaded()?
            || !self.find(ROOT, "wrapper")?.loaded.get()
        {
            return Err(sdk_error("native root not Loaded event/readback"));
        }
        self.flex_assertions()?;
        self.menu_assertions()?;
        let xaml = root.XamlRoot()?;
        for id in [
            "entry",
            "echo",
            "save",
            "check",
            "slider",
            "combo",
            "thumbnail",
        ] {
            let o = self.find(id, "control")?;
            if o.element.XamlRoot()? != xaml
                || !o.element.cast::<n::IFrameworkElement>()?.IsLoaded()?
            {
                return Err(sdk_error(format!("shared XamlRoot/Loaded {id}")));
            }
        }
        let objects: Vec<_> = self.objects.borrow().values().cloned().collect();
        let mut panels = 0;
        for o in &objects {
            if let Ok(panel) = o.element.cast::<n::IPanel>() {
                let list = panel.Children()?;
                let expected = o.children.borrow().clone();
                if list.Size()? as usize != expected.len() {
                    return Err(sdk_error("native child list size"));
                }
                for (i, child) in expected.iter().enumerate() {
                    if list.GetAt(i as u32)? != *child {
                        return Err(sdk_error("native keyed child order"));
                    }
                }
                panels += 1;
            }
        }
        for o in objects.iter().filter(|o| o.role == "control") {
            if let Kind::Tabs(tabs) = &o.node.kind {
                for id in o.node.children.borrow().iter() {
                    let page = self.view.find(id).map_err(sdk_error)?;
                    let expected = page.visible.get() && self.view.actionable_page(&page);
                    if self.find(id, "wrapper")?.element.Visibility()?
                        != if expected {
                            n::Visibility::Visible
                        } else {
                            n::Visibility::Collapsed
                        }
                    {
                        return Err(sdk_error("native selected page visibility"));
                    }
                    let header = self.find(id, "header")?;
                    let selected =
                        matches!(&page.kind,Kind::Page(key) if *key==*tabs.value.borrow());
                    let background = optional(header.element.cast::<n::IControl>()?.Background())?;
                    if selected {
                        if background
                            .ok_or_else(|| sdk_error("selected header background missing"))?
                            .cast::<n::ISolidColorBrush>()?
                            .Color()?
                            != (n::Color {
                                a: 255,
                                r: 40,
                                g: 100,
                                b: 190,
                            })
                        {
                            return Err(sdk_error("native selected header background"));
                        }
                    } else if background != header.background {
                        return Err(sdk_error("native unselected header background restoration"));
                    }
                }
            }
        }
        for id in ["table", "table-readonly"] {
            let o = self.find(id, "control")?;
            let Kind::Table(table) = &o.node.kind else {
                return Err(sdk_error("native table type"));
            };
            let data = table
                .data
                .borrow()
                .clone()
                .ok_or_else(|| sdk_error("table snapshot missing"))?;
            for (i, column) in data.columns.iter().enumerate() {
                if self
                    .find(id, &format!("column:{i}"))?
                    .element
                    .cast::<n::ITextBlock>()?
                    .Text()?
                    != column.title
                {
                    return Err(sdk_error("native table header text"));
                }
            }
            for (cell_id, cell) in table.cells.borrow().iter() {
                let role = if table.selection.is_some() {
                    "caption"
                } else {
                    "readonly-caption"
                };
                if self
                    .find(cell_id, role)?
                    .element
                    .cast::<n::ITextBlock>()?
                    .Text()?
                    != *cell.text.borrow()
                {
                    return Err(sdk_error("native table cell text"));
                }
            }
        }
        let entry = self.find("entry", "control")?;
        if entry.element.cast::<n::ITextBox>()?.Text()?
            != self.view.text("entry").unwrap_or_default()
        {
            return Err(sdk_error("native TextBox retained readback"));
        }
        if self
            .find("echo", "control")?
            .element
            .cast::<n::ITextBlock>()?
            .Text()?
            != self.view.text("echo").unwrap_or_default()
        {
            return Err(sdk_error("native TextBlock echo readback"));
        }
        if !self
            .find("readonly", "control")?
            .element
            .cast::<n::ITextBox>()?
            .IsReadOnly()?
        {
            return Err(sdk_error("native readonly"));
        }
        if self
            .find("check", "control")?
            .element
            .cast::<n::IToggleButton>()?
            .IsChecked()?
            != self.view.checked("check").unwrap_or_default()
        {
            return Err(sdk_error("native checked"));
        }
        if self
            .find("slider", "control")?
            .element
            .cast::<n::IRangeBase>()?
            .Value()?
            != self.view.slider_value("slider").unwrap_or_default()
        {
            return Err(sdk_error("native slider"));
        }
        if self
            .find("meter", "control")?
            .element
            .cast::<n::IRangeBase>()?
            .Value()?
            != self.view.meter_value("meter").unwrap_or_default()
        {
            return Err(sdk_error("native progress"));
        }
        let combo = self.find("combo", "control")?;
        let Kind::Combo(_, choices, selected) = &combo.node.kind else {
            return Err(sdk_error("combo type"));
        };
        if combo.element.cast::<n::ISelector>()?.SelectedIndex()?
            != choices
                .borrow()
                .iter()
                .position(|c| c.id == *selected.borrow())
                .map_or(-1, |i| i as i32)
        {
            return Err(sdk_error("native combo"));
        }
        let bitmap = self
            .find("thumbnail", "control")?
            .element
            .cast::<n::IImage>()?
            .Source()?
            .cast::<n::IBitmapSource>()?;
        if bitmap.PixelWidth()? != 1 || bitmap.PixelHeight()? != 1 {
            return Err(sdk_error("decoded PNG dimensions"));
        }
        let echo = self.find("echo", "control")?;
        let expected = echo.node.style.get().text.unwrap_or_default();
        let size = f64::from(expected.scale)
            * match expected.role {
                TextRole::Heading => 28.0,
                TextRole::Subtitle => 20.0,
                _ => 14.0,
            };
        if echo.element.cast::<n::ITextBlock>()?.FontSize()? != size {
            return Err(sdk_error("native font style"));
        }
        let style = echo.node.style.get();
        let wrapper = self
            .find("echo", "wrapper")?
            .element
            .cast::<n::IFrameworkElement>()?;
        if !auto_equal(wrapper.Width()?, style.placement.width.map(f64::from))
            || !auto_equal(wrapper.Height()?, style.placement.height.map(f64::from))
            || wrapper.Margin()? != thickness(style.margin)
            || self
                .find("echo", "padding")?
                .element
                .cast::<n::IBorder>()?
                .Padding()?
                != thickness(style.padding)
        {
            return Err(sdk_error("native placement/margin/padding style"));
        }
        let horizontal = match style.horizontal_alignment.unwrap_or(Alignment::Stretch) {
            Alignment::Start => n::HorizontalAlignment::Left,
            Alignment::Center => n::HorizontalAlignment::Center,
            Alignment::End => n::HorizontalAlignment::Right,
            Alignment::Stretch => n::HorizontalAlignment::Stretch,
        };
        let vertical = match style.vertical_alignment.unwrap_or(Alignment::Stretch) {
            Alignment::Start => n::VerticalAlignment::Top,
            Alignment::Center => n::VerticalAlignment::Center,
            Alignment::End => n::VerticalAlignment::Bottom,
            Alignment::Stretch => n::VerticalAlignment::Stretch,
        };
        if wrapper.HorizontalAlignment()? != horizontal || wrapper.VerticalAlignment()? != vertical
        {
            return Err(sdk_error("native alignment set/clear"));
        }
        let canvas_child = self.find("canvas-label", "wrapper")?.element.clone();
        if n::Canvas::GetLeft(&canvas_child)? != 12.0 || n::Canvas::GetTop(&canvas_child)? != 4.0 {
            return Err(sdk_error("native Canvas attached position"));
        }
        let grid_child = self
            .find("spanned-label", "wrapper")?
            .element
            .cast::<n::FrameworkElement>()?;
        if n::Grid::GetRow(&grid_child)? != 1
            || n::Grid::GetColumn(&grid_child)? != 0
            || n::Grid::GetRowSpan(&grid_child)? != 2
            || n::Grid::GetColumnSpan(&grid_child)? != 2
        {
            return Err(sdk_error("native Grid attached placement/span"));
        }
        println!(
            "NATIVE_LAYOUT_PASS generation={} Canvas-left-top=true Grid-row-column-span=true alignment-set-clear=true",
            self.generation
        );
        let foreground = optional(echo.element.cast::<n::ITextBlock>()?.Foreground())?;
        if let Some([r, g, b]) = expected.foreground {
            if foreground
                .ok_or_else(|| sdk_error("foreground missing"))?
                .cast::<n::ISolidColorBrush>()?
                .Color()?
                != (n::Color { a: 255, r, g, b })
            {
                return Err(sdk_error("native foreground style"));
            }
        } else if foreground != echo.foreground {
            return Err(sdk_error("native foreground default restoration"));
        }
        println!(
            "NATIVE_CHILDREN_PASS generation={} panels={panels} actual-order=true tab-selected-visibility-background=true table-header-cell-text=true",
            self.generation
        );
        println!(
            "NATIVE_PROPERTIES_PASS generation={} Loaded=true shared-XamlRoot=true TextBox=true TextBlock=true readonly=true checked=true slider=true combo=true progress=true PNG=1x1 font={size} text-events={}",
            self.generation,
            entry.changes.get()
        );
        Ok(())
    }
    fn flex_assertions(&self) -> Result<()> {
        let panel = self.find("flex", "control")?.element.clone();
        let wrapper = self.find("flex", "wrapper")?.element.clone();
        // Flush native invalidation, never call the overrides directly. These
        // facts must be produced by XAML's actual Measure/Arrange dispatch.
        wrapper.UpdateLayout()?;
        let wide = self
            .view
            .retained_style("flex")
            .and_then(|s| s.placement.width)
            == Some(288.0);
        let visible = self.view.find("flex-c").map_err(sdk_error)?.visible.get();
        let expected_width = if wide {
            if visible { 224.0 } else { 192.0 }
        } else {
            152.0
        };
        let expected_height = if !wide && visible { 47.0 } else { 30.0 };
        let close = |a: f32, b: f32| (a - b).abs() < 0.75;
        let desired = panel.DesiredSize()?;
        let render = panel.RenderSize()?;
        let offset = panel.ActualOffset()?;
        if !close(desired.width, expected_width)
            || !close(desired.height, expected_height)
            || !close(render.width, if wide { 260.0 } else { 160.0 })
            || !close(render.height, expected_height)
            || !close(offset.x, 11.0)
            || !close(offset.y, 13.0)
        {
            return Err(sdk_error(format!(
                "native Flex extent/padding wide={wide} visible={visible} desired={desired:?} render={render:?} offset={offset:?}"
            )));
        }
        let dimensions = [
            ("flex-a", 2.0, 1.0, 60.0, 20.0),
            ("flex-b", 72.0, 0.0, if wide { 120.0 } else { 80.0 }, 30.0),
            (
                "flex-c",
                if wide { 199.0 } else { 0.0 },
                if wide { 0.0 } else { 37.0 },
                25.0,
                10.0,
            ),
        ];
        for (id, x, y, width, height) in dimensions {
            let element = self.find(id, "wrapper")?.element.clone();
            if id == "flex-c" && !visible {
                if element.Visibility()? != n::Visibility::Collapsed {
                    return Err(sdk_error("native Flex collapsed child"));
                }
                continue;
            }
            let offset = element.ActualOffset()?;
            let size = element.RenderSize()?;
            if !close(offset.x, x)
                || !close(offset.y, y)
                || !close(size.width, width)
                || !close(size.height, height)
            {
                return Err(sdk_error(format!(
                    "native Flex slot {id}: offset={offset:?} size={size:?}"
                )));
            }
        }
        let outer = wrapper.RenderSize()?;
        let a = self.find("flex-a", "wrapper")?.element.DesiredSize()?;
        if !close(outer.height, expected_height + 32.0)
            || !close(a.width, 65.0)
            || !close(a.height, 25.0)
        {
            return Err(sdk_error("native Flex wrapper margin/padding dimensions"));
        }
        println!(
            "NATIVE_FLEX_PASS generation={} wide={wide} visible={visible} variable-width=true gap=7 margin-padding=true native-Measure-Arrange=true",
            self.generation
        );
        Ok(())
    }
}
enum Event {
    Click,
    Text,
    Check,
    Value,
    Selection,
    Dialog(n::ContentDialogResult),
}
fn sync_items<T: windows_core::RuntimeType + PartialEq>(
    list: &windows_collections::IVector<T>,
    items: &[T],
) -> Result<()>
where
    for<'a> &'a T: windows_core::Param<T>,
{
    let mut changed = list.Size()? as usize != items.len();
    if !changed {
        for (i, item) in items.iter().enumerate() {
            if list.GetAt(i as u32)? != *item {
                changed = true;
                break;
            }
        }
    }
    if changed {
        list.Clear()?;
        for item in items {
            list.Append(item)?;
        }
    }
    Ok(())
}
fn boxed(value: &str) -> Result<IInspectable> {
    windows_reference::IReference::<windows_core::HSTRING>::from(windows_core::HSTRING::from(value))
        .cast()
}
fn optional<T>(value: Result<T>) -> Result<Option<T>> {
    match value {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.code().is_ok() => Ok(None),
        Err(e) => Err(e),
    }
}
fn auto_equal(actual: f64, expected: Option<f64>) -> bool {
    expected.map_or_else(|| actual.is_nan(), |v| actual == v)
}
fn thickness(e: Edges) -> n::Thickness {
    n::Thickness {
        left: f64::from(e.left),
        top: f64::from(e.top),
        right: f64::from(e.right),
        bottom: f64::from(e.bottom),
    }
}
fn length(value: f64, grid_unit_type: n::GridUnitType) -> n::GridLength {
    n::GridLength {
        value,
        grid_unit_type,
    }
}
fn track_end(row: i32, span: i32) -> Result<i32> {
    row.checked_add(span)
        .filter(|end| row >= 0 && span > 0 && *end as usize <= super::MAX_NODES)
        .ok_or_else(|| sdk_error("native Grid row track overflow/budget"))
}
fn fills_height(view: &View, node: &Node, depth: usize) -> Result<bool> {
    if depth > super::MAX_DEPTH {
        return Err(sdk_error("native fill intent depth"));
    }
    let style = node.style.get();
    if !node.visible.get() || !view.actionable_page(node) || style.placement.height.is_some() {
        return Ok(false);
    }
    if style.vertical_alignment == Some(Alignment::Stretch) {
        return Ok(true);
    }
    match node.kind {
        Kind::Scroll(_) | Kind::Table(_) | Kind::Tabs(_) => Ok(true),
        Kind::Panel(_) | Kind::Page(_) => {
            for id in node.children.borrow().iter() {
                let child = view.find(id).map_err(sdk_error)?;
                if fills_height(view, &child, depth + 1)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}
fn grid_definitions(
    grid: &n::IGrid,
    columns: &[n::GridLength],
    rows: &[n::GridLength],
) -> Result<()> {
    let cs = grid.ColumnDefinitions()?;
    let mut changed = cs.Size()? as usize != columns.len();
    if !changed {
        for (i, c) in columns.iter().enumerate() {
            if cs.GetAt(i as u32)?.Width()? != *c {
                changed = true;
                break;
            }
        }
    }
    if changed {
        cs.Clear()?;
        for width in columns {
            let c = n::ColumnDefinition::new()?;
            c.SetWidth(*width)?;
            cs.Append(&c)?;
        }
    }
    let rs = grid.RowDefinitions()?;
    let mut changed = rs.Size()? as usize != rows.len();
    if !changed {
        for (i, r) in rows.iter().enumerate() {
            if rs.GetAt(i as u32)?.Height()? != *r {
                changed = true;
                break;
            }
        }
    }
    if changed {
        rs.Clear()?;
        for height in rows {
            let r = n::RowDefinition::new()?;
            r.SetHeight(*height)?;
            rs.Append(&r)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use panechord_runtime::{Binding, LayoutView, TextView};
    #[test]
    fn scroll_fill_intent_propagates_through_stacks_but_not_fixed_or_hidden_children() {
        let view = View::new(Binding::constant("layout".into()));
        let root = view.root();
        let stack = view.try_layout(&root, "stack", Layout::default()).unwrap();
        view.try_scroll(&stack, "scroll", false).unwrap();
        view.label(&stack, "footer", Binding::constant("pinned".into()));
        let node = |id| view.find(id).unwrap();
        assert!(fills_height(&view, &node(ROOT), 0).unwrap());
        assert!(fills_height(&view, &node("stack"), 1).unwrap());
        assert!(!fills_height(&view, &node("footer"), 2).unwrap());
        view.visible("scroll", Binding::constant(false));
        assert!(!fills_height(&view, &node("stack"), 1).unwrap());
        view.visible("scroll", Binding::constant(true));
        view.style(
            "scroll",
            panechord_runtime::Style {
                placement: panechord_runtime::Placement {
                    height: Some(100.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert!(!fills_height(&view, &node("stack"), 1).unwrap());
        view.style("scroll", panechord_runtime::Style::default());
        assert!(fills_height(&view, &node("stack"), 1).unwrap());
        assert!(fills_height(&view, &node(ROOT), super::super::MAX_DEPTH + 1).is_err());
    }
    #[test]
    fn auto_dimensions_clear_to_nan() {
        assert!(auto_equal(f64::NAN, None));
        assert!(!auto_equal(0.0, None));
        assert!(auto_equal(12.0, Some(12.0)));
        assert!(!auto_equal(f64::NAN, Some(12.0)));
    }
    #[test]
    fn synthetic_key_follows_node_not_logical_id() {
        let first = Node::new("same".into(), None, 0, Kind::Label);
        let next = Node::new("same".into(), None, 0, Kind::Label);
        assert_eq!(
            Renderer::key(&first, "control"),
            Renderer::key(&first, "control")
        );
        assert_ne!(
            Renderer::key(&first, "control"),
            Renderer::key(&first, "caption")
        );
        assert_ne!(
            Renderer::key(&first, "control"),
            Renderer::key(&next, "control")
        );
    }
    #[test]
    fn grid_tracks_fail_before_overflow_or_unbounded_allocation() {
        assert_eq!(track_end(1, 2).unwrap(), 3);
        assert_eq!(
            track_end(0, super::super::MAX_NODES as i32).unwrap(),
            super::super::MAX_NODES as i32
        );
        assert!(track_end(i32::MAX, 1).is_err());
        assert!(track_end(8192, 1).is_err());
        assert!(track_end(-1, 1).is_err());
        assert!(track_end(0, 0).is_err());
    }
}
