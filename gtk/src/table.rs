use super::*;
use panechord_runtime::{Table, TableColumn, TableRow, TableSelection, TextOverflow};
use std::sync::Arc;

#[derive(Clone)]
struct MountedRow {
    /// Column size groups this row's widgets were added to. Live groups are
    /// replaced on a column-schema change, so removal must target the groups
    /// that own the widgets, not the current ones.
    groups: Vec<gtk4::SizeGroup>,
}

struct TableState {
    data: RefCell<Table>,
    rows: gio::ListStore,
    headers: gtk4::Box,
    separator: gtk4::Separator,
    empty: gtk4::Label,
    groups: RefCell<Vec<gtk4::SizeGroup>>,
    selection: Option<Binding<Option<TableSelection>>>,
    selected: RefCell<Option<TableSelection>>,
    mounted: RefCell<HashMap<String, MountedRow>>,
    list: RefCell<Option<gtk4::ListView>>,
    owner: WeakView,
}

pub(super) fn apply_text_style(label: &gtk4::Label, text: Option<panechord_runtime::TextStyle>) {
    label.remove_css_class("heading");
    label.remove_css_class("monospace");
    let Some(text) = text else {
        label.set_attributes(None);
        return;
    };
    let attributes = gtk4::pango::AttrList::new();
    match text.role {
        panechord_runtime::TextRole::Body => {}
        panechord_runtime::TextRole::Heading => label.add_css_class("heading"),
        panechord_runtime::TextRole::Code => {
            label.add_css_class("monospace");
            attributes.insert(gtk4::pango::AttrString::new_family("monospace"));
        }
        panechord_runtime::TextRole::Subtitle => {
            attributes.insert(gtk4::pango::AttrInt::new_weight(gtk4::pango::Weight::Bold));
        }
    }
    attributes.insert(gtk4::pango::AttrFloat::new_scale(f64::from(text.scale)));
    if let Some([red, green, blue]) = text.foreground {
        attributes.insert(gtk4::pango::AttrColor::new_foreground(
            u16::from(red) * 257,
            u16::from(green) * 257,
            u16::from(blue) * 257,
        ));
    }
    label.set_attributes(Some(&attributes));
}

fn label(column: &TableColumn, value: &str, heading: bool) -> gtk4::Label {
    let label = gtk4::Label::new(Some(value));
    configure_label(&label, column, heading);
    label
}

fn configure_label(label: &gtk4::Label, column: &TableColumn, heading: bool) {
    label.set_xalign(0.0);
    label.set_hexpand(column.stretch);
    label.set_halign(gtk4::Align::Fill);
    label.set_valign(gtk4::Align::Start);
    if let Some(width) = column.width {
        label.set_size_request(width.round() as i32, -1);
    }
    apply_text_style(
        label,
        Some(if heading {
            panechord_runtime::TextStyle {
                role: panechord_runtime::TextRole::Heading,
                ..Default::default()
            }
        } else {
            column.text
        }),
    );
    match column.overflow {
        TextOverflow::Wrap if !heading => {
            label.set_wrap(true);
            label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
            label.set_max_width_chars(1);
        }
        TextOverflow::EllipsizeMiddle if !heading => {
            label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle)
        }
        _ => label.set_ellipsize(gtk4::pango::EllipsizeMode::End),
    }
}

fn set_selected(button: &gtk4::ToggleButton, active: bool) {
    button.set_active(active);
    if active {
        button.remove_css_class("flat");
        button.add_css_class("suggested-action");
    } else {
        button.remove_css_class("suggested-action");
        button.add_css_class("flat");
    }
}

impl TableState {
    /// Release a row body's children from the column size groups they were
    /// added to, then detach them. Size groups hold strong widget references:
    /// without removal every rebind leaks the previous row widgets into the
    /// group, and the group later measures finalized (dead) widgets —
    /// crashing a later layout, teardown, or reopen of the same view.
    /// Removal targets the groups stored with the mounted row: after a
    /// column-schema change the live groups no longer own the recycled
    /// widgets, and removing them from the wrong group warns
    /// (`gtk_size_group_remove_widget` assertion) without releasing anything.
    fn clear_body(&self, body: &gtk4::Box) {
        let key = body.widget_name();
        let entry = self.mounted.borrow_mut().remove(key.as_str());
        let mut child = body.first_child();
        let mut index = 0;
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Some(groups) = entry.as_ref().map(|row| &row.groups)
                && let Some(group) = groups.get(index)
            {
                group.remove_widget(&widget);
            }
            body.remove(&widget);
            index += 1;
        }
    }

    fn select(&self, selected: Option<TableSelection>) {
        let validation = selected
            .as_ref()
            .map(|selection| selection.validate(&self.data.borrow()));
        if let Some(Err(error)) = validation {
            if let Some(view) = self.owner.upgrade() {
                view.report_error(error.into());
            }
            return;
        }
        self.selected.replace(selected.clone());
        self.paint(&selected);
    }

    /// Paint the pressed state onto the live row widgets. The mount map
    /// can lag ListView recycling, so selection visuals walk the widget
    /// tree instead of trusting it; the map stays for size-group release.
    fn paint(&self, selected: &Option<TableSelection>) {
        let Some(list) = self.list.borrow().clone() else {
            return;
        };
        let mut row = list.first_child();
        while let Some(row_widget) = row {
            let body = row_widget
                .first_child()
                .and_then(|child| child.downcast::<gtk4::Box>().ok());
            if let Some(body) = body {
                let key = body.widget_name().to_string();
                let mut cell = body.first_child();
                let mut column = 0;
                while let Some(widget) = cell {
                    if let Ok(button) = widget.clone().downcast::<gtk4::ToggleButton>() {
                        set_selected(
                            &button,
                            selected.as_ref().is_some_and(|selection| {
                                selection.row_key == key && selection.column == column
                            }),
                        );
                        column += 1;
                    }
                    cell = widget.next_sibling();
                }
            }
            row = row_widget.next_sibling();
        }
    }

    fn replace(&self, data: Table) {
        let previous = self.data.replace(data.clone());
        self.empty.set_text(&data.empty_text);
        self.empty.set_visible(data.rows.is_empty());
        let headers = data.columns.iter().any(|column| !column.title.is_empty());
        self.headers.set_visible(headers);
        self.separator.set_visible(headers);
        let columns_changed =
            previous.columns != data.columns || previous.column_gap != data.column_gap;
        if columns_changed {
            while let Some(child) = self.headers.first_child() {
                self.headers.remove(&child);
            }
            self.headers.set_spacing(data.column_gap.round() as i32);
            let groups: Vec<_> = data
                .columns
                .iter()
                .map(|column| {
                    let group = gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal);
                    let header = label(column, &column.title, true);
                    group.add_widget(&header);
                    self.headers.append(&header);
                    group
                })
                .collect();
            self.groups.replace(groups);
        }
        if columns_changed || previous.rows != data.rows {
            let mut header = self.headers.first_child();
            for (index, column) in data.columns.iter().enumerate() {
                let widget = header.take().unwrap();
                header = widget.next_sibling();
                if column.width.is_none() && !column.stretch {
                    let sample = label(column, "", false);
                    let layout = sample.layout();
                    layout.set_width(-1);
                    layout.set_ellipsize(gtk4::pango::EllipsizeMode::None);
                    let width = data.rows.iter().fold(0, |width, row| {
                        layout.set_text(&row.cells[index].text);
                        width.max(layout.pixel_size().0)
                    });
                    let width = if self.selection.is_some() {
                        sample.set_size_request(width, -1);
                        let button = gtk4::ToggleButton::new();
                        button.add_css_class("flat");
                        button.add_css_class("text-button");
                        button.set_child(Some(&sample));
                        button.measure(gtk4::Orientation::Horizontal, -1).1
                    } else {
                        width
                    };
                    widget.set_size_request(width, -1);
                }
            }
        }
        let mut prefix = 0;
        let mut suffix = 0;
        if !columns_changed
            && previous.row_gap == data.row_gap
            && previous.row_inset == data.row_inset
        {
            while prefix < previous.rows.len().min(data.rows.len())
                && previous.rows[prefix] == data.rows[prefix]
            {
                prefix += 1;
            }
            while suffix < previous.rows.len().min(data.rows.len()) - prefix
                && previous.rows[previous.rows.len() - 1 - suffix]
                    == data.rows[data.rows.len() - 1 - suffix]
            {
                suffix += 1;
            }
        }
        let objects: Vec<_> = data.rows[prefix..data.rows.len() - suffix]
            .iter()
            .map(|row| glib::BoxedAnyObject::new(row.clone()))
            .collect();
        self.rows.splice(
            prefix as u32,
            (previous.rows.len() - prefix - suffix) as u32,
            &objects,
        );
        if let Some(selection) = &self.selection {
            self.select(selection.get());
        }
    }
}

impl View {
    pub fn table(&self, parent: &Parent, id: &str, data: Binding<Table>) {
        self.table_impl(parent, id, data, None);
    }

    pub fn table_with_selection(
        &self,
        parent: &Parent,
        id: &str,
        data: Binding<Table>,
        selection: Binding<Option<TableSelection>>,
    ) {
        self.table_impl(parent, id, data, Some(selection));
    }

    fn table_impl(
        &self,
        parent: &Parent,
        id: &str,
        data: Binding<Table>,
        selection: Option<Binding<Option<TableSelection>>>,
    ) {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let headers = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        root.append(&headers);
        root.append(&separator);
        let state = Rc::new(TableState {
            data: RefCell::new(Table::default()),
            rows: gio::ListStore::new::<glib::BoxedAnyObject>(),
            headers,
            separator,
            empty: gtk4::Label::new(None),
            groups: RefCell::new(Vec::new()),
            selection: selection.clone(),
            selected: RefCell::new(None),
            mounted: RefCell::new(HashMap::new()),
            list: RefCell::new(None),
            owner: self.downgrade(),
        });
        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(move |_, object| {
            let item = object.downcast_ref::<gtk4::ListItem>().unwrap();
            item.set_child(Some(&gtk4::Box::new(gtk4::Orientation::Horizontal, 0)));
        });
        let weak = Rc::downgrade(&state);
        let row_css = gtk4::CssProvider::new();
        row_css.load_from_data("row { padding: 0; }");
        factory.connect_bind(move |_, object| {
            let Some(state) = weak.upgrade() else { return };
            let item = object.downcast_ref::<gtk4::ListItem>().unwrap();
            let row = item
                .item()
                .unwrap()
                .downcast::<glib::BoxedAnyObject>()
                .unwrap();
            let row = row.borrow::<Arc<TableRow>>();
            let body = item.child().unwrap().downcast::<gtk4::Box>().unwrap();
            // Pops the previous mount entry and releases its widgets from
            // the groups that own them (see `clear_body`).
            state.clear_body(&body);
            let data = state.data.borrow().clone();
            body.set_spacing(data.column_gap.round() as i32);
            let inset = data.row_inset.round() as i32;
            body.set_margin_top(inset);
            body.set_margin_start(inset);
            body.set_margin_end(inset);
            body.set_margin_bottom(data.row_gap.round() as i32 + inset);
            body.set_widget_name(&row.key);
            for (index, (column, cell)) in data.columns.iter().zip(&row.cells).enumerate() {
                let mut column = column.clone();
                column.text.foreground = cell.foreground.or(column.text.foreground);
                if let Some(selection) = &state.selection {
                    let width = column.width.take();
                    let button = gtk4::ToggleButton::with_label(&cell.text);
                    let label = button.child().unwrap().downcast::<gtk4::Label>().unwrap();
                    configure_label(&label, &column, false);
                    label.set_valign(gtk4::Align::Center);
                    button.set_visible(!column.hide_empty || !cell.text.is_empty());
                    button.add_css_class("flat");
                    button.add_css_class("text-button");
                    button.set_hexpand(column.stretch);
                    if let Some(width) = width {
                        button.set_size_request(width.round() as i32, -1);
                    }
                    set_selected(
                        &button,
                        state.selected.borrow().as_ref().is_some_and(|selected| {
                            selected.row_key == row.key && selected.column == index
                        }),
                    );
                    let weak = Rc::downgrade(&state);
                    let writer = selection.clone();
                    let selected = TableSelection {
                        row_key: row.key.clone(),
                        column: index,
                    };
                    button.connect_clicked(move |button| {
                        let Some(state) = weak.upgrade() else { return };
                        if state.owner.upgrade().is_none() {
                            return;
                        }
                        let valid = selected.validate(&state.data.borrow()).is_ok();
                        // Detached widgets have no root: this rejects clicks
                        // from recycled buttons without relying on the mount
                        // map, which can lag ListView recycling and would
                        // otherwise swallow legitimate clicks with no way
                        // back. (The map is still maintained for size-group
                        // release.)
                        let rooted = button.root().is_some();
                        if !(valid
                            && rooted
                            && button.is_visible()
                            && button.is_sensitive()
                            && writer.is_writable())
                        {
                            let active = *state.selected.borrow() == Some(selected.clone());
                            set_selected(button, active);
                            return;
                        }
                        panechord_runtime::ignore_write(writer.try_set(Some(selected.clone())));
                        // Immediate visual sync: the binding echoes back
                        // asynchronously, and same-row changes trigger no
                        // data refresh that would repaint us. Paint from the
                        // live widget tree rather than the mount map, which
                        // can lag ListView recycling.
                        state.paint(&Some(selected.clone()));
                    });
                    state.groups.borrow()[index].add_widget(&button);
                    body.append(&button);
                } else {
                    let label = label(&column, &cell.text, false);
                    label.set_visible(!column.hide_empty || !cell.text.is_empty());
                    label.set_selectable(true);
                    state.groups.borrow()[index].add_widget(&label);
                    body.append(&label);
                }
            }
            state.mounted.borrow_mut().insert(
                row.key.clone(),
                MountedRow {
                    groups: state.groups.borrow().clone(),
                },
            );
        });
        let weak = Rc::downgrade(&state);
        factory.connect_unbind(move |_, object| {
            let item = object.downcast_ref::<gtk4::ListItem>().unwrap();
            if let Some(body) = item
                .child()
                .and_then(|child| child.downcast::<gtk4::Box>().ok())
            {
                if let Some(state) = weak.upgrade() {
                    state.clear_body(&body);
                } else {
                    while let Some(child) = body.first_child() {
                        body.remove(&child);
                    }
                }
            }
        });
        let native_selection = gtk4::NoSelection::new(Some(state.rows.clone()));
        let list = gtk4::ListView::new(Some(native_selection), Some(factory));
        // Padding reset for the internal row widgets: attach once to the
        // ListView (providers inherit down to the rows). Never touch the
        // internal row widgets per bind: provider churn on ListView-managed
        // widgets corrupts the item tracker and crashes a later teardown or
        // reopen (notification history, second open).
        list.style_context()
            .add_provider(&row_css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
        list.set_vexpand(true);
        list.set_hexpand(true);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_vexpand(true);
        scroll.set_hexpand(true);
        scroll.set_child(Some(&list));
        state.list.replace(Some(list.clone()));
        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&scroll));
        state.empty.set_halign(gtk4::Align::Center);
        state.empty.set_valign(gtk4::Align::Center);
        overlay.add_overlay(&state.empty);
        overlay.set_vexpand(true);
        root.append(&overlay);
        root.set_vexpand(true);
        self.mount_native(parent, id, &root);
        let weak = Rc::downgrade(&state);
        let owner = self.downgrade();
        self.bind_native(id, data, &root, move |_, data| {
            if let Some(state) = weak.upgrade() {
                match data.validate() {
                    Ok(()) => state.replace(data),
                    Err(error) => {
                        if let Some(view) = owner.upgrade() {
                            view.report_error(error.into());
                        }
                    }
                }
            }
        });
        if let Some(selection) = selection {
            let weak = Rc::downgrade(&state);
            self.keep_at(
                id,
                selection.subscribe(Rc::new(move |selected| {
                    if let Some(state) = weak.upgrade() {
                        state.select(selected);
                    }
                })),
            );
        }
        self.keep_at(id, Subscription::new(move || drop(state)));
    }
}
