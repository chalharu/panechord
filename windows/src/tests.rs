//! Pure retained logic / LocalPool. Never starts App, COM, a host GUI or HWND.
use super::{Host, Parent, View};
use futures::{
    executor::{LocalPool, LocalSpawner},
    future::LocalBoxFuture,
    task::LocalSpawnExt,
};
use panechord_runtime::{
    AutomationView, Binding, ButtonView, Choice, DialogFooter, DialogView, Executor, FooterButton,
    ImageFrame, Keyed, Layout, LayoutView, ListView, MediaView, MenuItem, MenuNode, MenuRole,
    MenuView, Placement, PointerPosition, SelectView, Session, ShutdownPhase, SliderRange, Store,
    Style, Subscription, TabPosition, TabView, Table, TableCell, TableColumn, TableRow,
    TableSelection, TableView, Template, TextOverflow, TextRole, TextStyle, TextView, UiHost,
    ViewCore, Viewport,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

struct LocalExecutor(LocalSpawner);
impl Executor for LocalExecutor {
    fn spawn(&self, future: LocalBoxFuture<'static, ()>) {
        self.0.spawn_local(future).unwrap();
    }
}
fn setup() -> (LocalPool, Rc<dyn Executor>, Host) {
    let pool = LocalPool::new();
    let executor: Rc<dyn Executor> = Rc::new(LocalExecutor(pool.spawner()));
    let host = Host::new(executor.clone());
    (pool, executor, host)
}
fn view() -> View {
    View::new(Binding::constant("Next".into()))
}
type Listener<T> = Rc<RefCell<Option<Rc<dyn Fn(T)>>>>;
fn source<T: Clone + 'static>(value: T) -> (Binding<T>, Listener<T>, Rc<Cell<usize>>) {
    let initial = value.clone();
    let listener: Listener<T> = Rc::default();
    let target = listener.clone();
    let drops = Rc::new(Cell::new(0));
    let dropped = drops.clone();
    let binding = Binding::external(
        move || value.clone(),
        None,
        move |callback| {
            target.replace(Some(callback.clone()));
            callback(initial.clone());
            let target = target.clone();
            let dropped = dropped.clone();
            Subscription::new(move || {
                target.take();
                dropped.set(dropped.get() + 1);
            })
        },
    );
    (binding, listener, drops)
}
fn emit<T: Clone>(listener: &Listener<T>, value: T) {
    let callback = listener.borrow().as_ref().unwrap().clone();
    callback(value);
}
fn assert_full_view<V: panechord_runtime::View>() {}

#[test]
fn shared_macro_root_accepts_appearance_and_remains_reserved() {
    let view = view();
    let style = Style {
        padding: panechord_runtime::Edges {
            left: 16.0,
            top: 16.0,
            right: 16.0,
            bottom: 16.0,
        },
        ..Default::default()
    };
    view.try_style("__root", style).unwrap();
    assert_eq!(view.retained_style("__root"), Some(style));
    view.style_binding("__root", Binding::constant(Style::default()));
    view.visible("__root", Binding::constant(true));
    view.enabled("__root", Binding::constant(true));
    assert_eq!(view.retained_style("__root"), Some(Style::default()));
    assert!(
        view.try_layout(&view.root(), "__root", Layout::default())
            .is_err()
    );
    assert_eq!(view.node_count(), 0);
    assert_eq!(view.error(), None);
}

#[test]
fn twelve_capability_contract_is_real_and_core_callbacks_stay_stable() {
    assert_full_view::<View>();
    let (_, executor, _) = setup();
    let model = Store::new(String::from("accepted"), executor);
    let text = model.bind(Clone::clone, Some(|s, value| *s = value));
    let view = View::new(text.clone());
    view.entry_with_placeholder(
        &view.root(),
        "entry",
        text,
        Binding::constant("hint".into()),
    );
    view.label(&view.root(), "label", Binding::constant("caption".into()));
    let callback = view.text_changed_callback("entry").unwrap();
    let initial = view.render();
    assert_eq!(initial, view.render());
    assert_eq!(callback, view.text_changed_callback("entry").unwrap());
    assert!(callback.call("changed".into()));
    assert_eq!(view.text("entry").as_deref(), Some("changed"));
    assert_eq!(view.title(), "changed");
    assert_eq!(model.snapshot(), "changed");
    view.close();
    let _ = callback.call("after-close".into());
    assert_eq!(model.snapshot(), "changed");
    assert!(view.downgrade().upgrade().is_none());
}
#[test]
fn readonly_rejections_restore_editor_without_a_native_echo_loop() {
    let view = view();
    view.entry(&view.root(), "entry", Binding::constant("accepted".into()));
    let before = view.render();
    assert!(
        view.edit("entry", "rejected")
            .unwrap_err()
            .contains("read-only")
    );
    assert_eq!(view.text("entry").as_deref(), Some("accepted"));
    let recovered = view.render();
    assert_ne!(before, recovered);
    assert!(view.native_text_changed("entry", "accepted").is_ok());
    assert_eq!(recovered, view.render());
}
#[test]
fn parent_identity_foreign_and_duplicate_ids_never_mutate_the_old_tree() {
    let view = view();
    let other = View::new(Binding::constant("other".into()));
    assert!(
        view.try_layout(&other.root(), "bad", Layout::default())
            .is_err()
    );
    view.label(&view.root(), "label", Binding::constant("old".into()));
    view.label(&view.root(), "label", Binding::constant("new".into()));
    assert_eq!(view.node_count(), 1);
    assert_eq!(view.text("label").as_deref(), Some("old"));
    let parent = view.root();
    view.close();
    assert!(view.try_layout(&parent, "late", Layout::default()).is_err());
}
#[derive(Clone)]
struct SelectModel {
    checked: bool,
    selected: String,
    slider: f64,
    enabled: bool,
}
#[test]
fn native_and_automation_selection_paths_validate_and_follow_accepted_bindings() {
    let (mut pool, executor, _) = setup();
    let model = Store::new(
        SelectModel {
            checked: false,
            selected: "a".into(),
            slider: 2.0,
            enabled: true,
        },
        executor,
    );
    let view = view();
    let panel = view.layout(&view.root(), "panel", Layout::default());
    view.enabled("panel", model.bind(|s| s.enabled, None));
    view.checkbox(
        &panel,
        "check",
        Binding::constant("Check".into()),
        model.bind(|s| s.checked, Some(|s, v| s.checked = v)),
    );
    let selected = model.bind(|s| s.selected.clone(), Some(|s, v| s.selected = v));
    view.radio(
        &panel,
        "radio-a",
        Binding::constant("A".into()),
        selected.clone(),
        "a",
    );
    view.radio(
        &panel,
        "radio-b",
        Binding::constant("B".into()),
        selected.clone(),
        "b",
    );
    view.combo(
        &panel,
        "combo",
        Binding::constant(vec![
            Choice {
                id: "a".into(),
                label: "same".into(),
            },
            Choice {
                id: "b".into(),
                label: "same".into(),
            },
        ]),
        selected,
    );
    view.slider(
        &panel,
        "slider",
        model.bind(|s| s.slider, Some(|s, v| s.slider = v)),
        SliderRange {
            min: 0.0,
            max: 10.0,
            step: 2.0,
        },
    );
    let initial = view.render();
    assert_eq!(initial, view.render());
    assert!(view.checked_changed_callback("check").unwrap().call(true));
    assert!(
        view.selection_changed_callback("combo")
            .unwrap()
            .call(Some(1))
    );
    assert!(view.value_changed_callback("slider").unwrap().call(3.0));
    pool.run_until_stalled();
    assert!(model.snapshot().checked);
    assert_eq!(model.snapshot().selected, "b");
    assert_eq!(view.checked("radio-b"), Some(true));
    assert_eq!(view.slider_value("slider"), Some(4.0));
    assert!(view.slide("slider", f64::NAN).is_err());
    assert!(view.select("combo", "missing").is_err());
    assert_eq!(view.selected("combo").as_deref(), Some("b"));
    assert!(model.update(|s| s.enabled = false));
    assert!(view.toggle("check", false).is_err());
    assert!(view.select("radio-a", "a").is_err());
    assert!(view.slide("slider", 6.0).is_err());
    assert_eq!(view.error(), None);
}
#[test]
fn rejected_conversions_and_invalid_combo_snapshots_keep_last_valid_values() {
    let (_, executor, _) = setup();
    let model = Store::new(2.0, executor);
    let rejected = model
        .bind(|s| *s, Some(|s, v| *s = v))
        .convert(|v| v, |_| Err("conversion rejected".into()));
    let view = view();
    view.slider(
        &view.root(),
        "s",
        rejected,
        SliderRange {
            min: 0.0,
            max: 10.0,
            step: 1.0,
        },
    );
    let before = view.render();
    assert!(view.slide("s", 3.0).is_err());
    assert_eq!(view.slider_value("s"), Some(2.0));
    assert_ne!(before, view.render());
    let choices = vec![Choice {
        id: "a".into(),
        label: "A".into(),
    }];
    let (binding, listener, _) = source(choices.clone());
    view.combo(&view.root(), "c", binding, Binding::constant("a".into()));
    let accepted = view.render();
    emit(&listener, vec![choices[0].clone(), choices[0].clone()]);
    assert_eq!(view.selected("c").as_deref(), Some("a"));
    assert_eq!(accepted, view.render());
    assert!(view.error().is_some());
}
#[test]
fn image_is_encoded_png_not_raw_rgba_and_meter_is_finite_clamped() {
    let (_, executor, _) = setup();
    let pointer = Store::new(None::<PointerPosition>, executor);
    let pixels: Arc<[u8]> = vec![255, 0, 0, 255, 0, 255, 0, 128].into();
    let frame = ImageFrame::new(2, 1, pixels.clone()).unwrap();
    let (image_source, listener, _) = source(Some(frame.clone()));
    let view = view();
    view.image_with_pointer(
        &view.root(),
        "image",
        image_source,
        pointer.bind(|s| *s, Some(|s, v| *s = v)),
    );
    assert_eq!(view.image_update_count(), 1);
    let encoded = view.image_bytes("image").unwrap();
    assert!(encoded.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_ne!(encoded, pixels.as_ref());
    let mut reader = png::Decoder::new(std::io::Cursor::new(encoded))
        .read_info()
        .unwrap();
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!((info.width, info.height), (2, 1));
    assert_eq!(&output[..info.buffer_size()], pixels.as_ref());
    emit(&listener, Some(frame));
    assert_eq!(view.image_update_count(), 1);
    emit(&listener, None);
    assert!(view.image_bytes("image").is_none());
    assert_eq!(view.image_update_count(), 2);
    view.set_pointer("image", Some(PointerPosition { x: 1.0, y: 2.0 }))
        .unwrap();
    assert_eq!(pointer.snapshot().unwrap().x, 1.0);
    assert!(
        view.set_pointer(
            "image",
            Some(PointerPosition {
                x: f32::NAN,
                y: 0.0
            })
        )
        .is_err()
    );
    let (value, values, _) = source(2.0);
    view.meter(&view.root(), "meter", value);
    assert_eq!(view.meter_value("meter"), Some(1.0));
    emit(&values, -2.0);
    assert_eq!(view.meter_value("meter"), Some(0.0));
    emit(&values, f64::INFINITY);
    assert_eq!(view.meter_value("meter"), Some(0.0));
    assert!(view.error().unwrap().contains("finite"));
}
#[test]
fn layout_canvas_grid_style_separator_and_honest_flex_rejection() {
    let view = view();
    assert!(
        view.try_layout(&view.root(), "flex", Layout::Flex { gap: 0.0 })
            .unwrap_err()
            .contains("Flex")
    );
    let canvas = view
        .try_layout(&view.root(), "canvas", Layout::Absolute)
        .unwrap();
    view.label(&canvas, "code", Binding::constant("let x = 1;".into()));
    let unstyled = view.render();
    assert!(
        view.try_style(
            "code",
            Style {
                text: Some(TextStyle {
                    role: TextRole::Code,
                    ..Default::default()
                }),
                ..Default::default()
            }
        )
        .unwrap_err()
        .contains("font_family")
    );
    assert_eq!(unstyled, view.render());
    view.try_style(
        "code",
        Style {
            placement: Placement {
                x: 12.0,
                y: -4.0,
                width: Some(180.0),
                ..Default::default()
            },
            text: Some(TextStyle {
                role: TextRole::Heading,
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .unwrap();
    view.separator(&canvas, "line", true);
    let grid = view
        .try_layout(
            &view.root(),
            "grid",
            Layout::Grid {
                columns: 2,
                gap: 4.0,
            },
        )
        .unwrap();
    view.label(&grid, "grid-label", Binding::constant("grid".into()));
    let before = view.render();
    assert!(
        view.try_style(
            "grid-label",
            Style {
                placement: Placement {
                    column: 1,
                    column_span: 2,
                    ..Default::default()
                },
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(before, view.render());
    assert_eq!(view.error(), None);
    view.try_scroll(&view.root(), "scroll", true).unwrap();
}
#[test]
fn applicability_replacements_ignore_stale_callbacks_and_drop_tokens() {
    let view = view();
    view.entry(&view.root(), "entry", Binding::constant("text".into()));
    let (first, listener, drops) = source(true);
    view.visible("entry", first);
    let stale = listener.borrow().as_ref().unwrap().clone();
    let (second, next, next_drops) = source(false);
    view.visible("entry", second);
    assert_eq!(drops.get(), 1);
    let hidden = view.render();
    stale(true);
    assert_eq!(hidden, view.render());
    emit(&next, true);
    assert_ne!(hidden, view.render());
    view.close();
    assert_eq!(next_drops.get(), 1);
    stale(true);
    assert_eq!(view.node_count(), 0);
}
fn row_template(saved: Rc<RefCell<Vec<(View, Parent)>>>) -> Template<View, Parent, String> {
    Rc::new(move |view, parent, _, model| {
        saved.borrow_mut().push((view.clone(), parent.clone()));
        view.entry(
            parent,
            "edit",
            model.bind(Clone::clone, Some(|s, v| *s = v)),
        );
        view.label(parent, "label", model.bind(Clone::clone, None));
    })
}
#[test]
fn repeat_scoped_ids_store_identity_raii_and_no_resurrection() {
    let (_, executor, _) = setup();
    let first = Store::new("first".to_owned(), executor.clone());
    let second = Store::new("second".to_owned(), executor);
    let (items, listener, _) = source(vec![Keyed::new("one", first.clone())]);
    let saved = Rc::new(RefCell::new(Vec::new()));
    let view = view();
    view.repeat(
        &view.root(),
        "rows",
        items,
        2.0,
        row_template(saved.clone()),
    );
    assert_eq!(view.item_keys("rows"), vec!["one"]);
    assert_eq!(view.mounted_count("rows"), 1);
    assert_eq!(view.text("rows/one/label").as_deref(), Some("first"));
    let stale = view.text_changed_callback("rows/one/edit").unwrap();
    let old = saved.borrow()[0].clone();
    emit(&listener, vec![Keyed::new("one", first.clone())]);
    assert_eq!(saved.borrow().len(), 1);
    emit(&listener, vec![Keyed::new("one", second.clone())]);
    assert_eq!(saved.borrow().len(), 2);
    let _ = stale.call("stale".into());
    assert_eq!(first.snapshot(), "first");
    assert_eq!(second.snapshot(), "second");
    assert!(
        old.0
            .try_layout(&old.1, "resurrect", Layout::default())
            .is_err()
    );
    assert!(old.0.downgrade().upgrade().is_none());
    assert!(first.update(|s| *s = "old model update".into()));
    assert_eq!(view.text("rows/one/label").as_deref(), Some("second"));
    let accepted = view.render();
    emit(
        &listener,
        vec![
            Keyed::new("one", second.clone()),
            Keyed::new("one", second.clone()),
        ],
    );
    assert_eq!(accepted, view.render());
    assert!(view.items_error("rows").is_some());
    assert_eq!(view.mounted_count("rows"), 1);
    emit(&listener, vec![]);
    assert_eq!(view.mounted_count("rows"), 0);
    assert_eq!(view.node_count(), 1);
    assert!(!second.is_closed());
}
#[test]
fn viewport_mounts_are_bounded_and_content_some_none_releases_children() {
    let (_, executor, _) = setup();
    let items: Vec<_> = (0..600)
        .map(|index| {
            Keyed::new(
                index.to_string(),
                Store::new(index.to_string(), executor.clone()),
            )
        })
        .collect();
    let view = view();
    let saved = Rc::new(RefCell::new(Vec::new()));
    let (viewport, changes, _) = source(Viewport {
        count: 256,
        ..Default::default()
    });
    view.items(
        &view.root(),
        "list",
        Binding::constant(items.clone()),
        viewport,
        row_template(saved),
    );
    assert_eq!(view.item_keys("list").len(), 600);
    assert_eq!(view.mounted_count("list"), 256);
    view.viewport("list", 400);
    assert_eq!(view.mounted_count("list"), 256);
    assert!(view.text("list/0/label").is_none());
    let accepted = view.render();
    emit(
        &changes,
        Viewport {
            count: 257,
            ..Default::default()
        },
    );
    assert_eq!(accepted, view.render());
    assert!(view.items_error("list").is_some());
    let (content, changed, _) = source(Some(items[0].clone()));
    view.content(
        &view.root(),
        "content",
        content,
        Rc::new(|v, p, _, s| v.label(p, "label", s.bind(Clone::clone, None))),
    );
    assert_eq!(view.mounted_count("content"), 1);
    emit(&changed, None);
    assert_eq!(view.mounted_count("content"), 0);
    assert!(view.text("content/0/label").is_none());
}
#[test]
fn collection_callbacks_can_reenter_without_borrow_panics_and_scope_keep_is_raii() {
    let (_, executor, _) = setup();
    let item = Keyed::new("one", Store::new("one".to_owned(), executor));
    let (items, listener, _) = source(vec![item.clone()]);
    let late = listener.clone();
    let dropped = Rc::new(Cell::new(0));
    let count = dropped.clone();
    let view = view();
    view.repeat(
        &view.root(),
        "rows",
        items,
        0.0,
        Rc::new(move |v, p, _, s| {
            let count = count.clone();
            v.keep(Subscription::new(move || count.set(count.get() + 1)));
            v.label(p, "label", s.bind(Clone::clone, None));
            emit(&late, Vec::new());
        }),
    );
    assert_eq!(view.mounted_count("rows"), 0);
    assert_eq!(dropped.get(), 1);
    assert_eq!(view.error(), None);
}
#[test]
fn manual_and_generic_tabs_four_positions_and_readonly_fallback() {
    let view = view();
    let tabs = view.tab(&view.root(), "tabs", Binding::constant("missing".into()));
    let first = view
        .try_tab_page(&tabs, "first", Binding::constant("First".into()), "a")
        .unwrap();
    assert!(
        view.try_tab_page(&tabs, "duplicate", Binding::constant("Dup".into()), "a")
            .is_err()
    );
    assert!(
        view.try_tab_page(&tabs, "bad", Binding::constant("Bad".into()), "bad/key")
            .is_err()
    );
    let second = view.tab_page(&tabs, "second", Binding::constant("Second".into()), "b");
    view.label(&first, "a-text", Binding::constant("A".into()));
    view.entry(&second, "b-edit", Binding::constant("B".into()));
    view.finish_tab(&tabs).unwrap();
    assert_eq!(view.selected("tabs").as_deref(), Some("a"));
    assert!(view.select("tabs", "b").is_err());
    assert!(view.edit("b-edit", "hidden").is_err());
    let mut previous = view.render();
    for position in [
        TabPosition::Bottom,
        TabPosition::Left,
        TabPosition::Right,
        TabPosition::Top,
    ] {
        view.tab_position("tabs", position).unwrap();
        let next = view.render();
        assert_ne!(previous, next);
        previous = next;
    }
    let (_, executor, _) = setup();
    let selected = Store::new("one".to_owned(), executor.clone());
    let one = Keyed::new("one", Store::new("One".to_owned(), executor.clone()));
    let two = Keyed::new("two", Store::new("Two".to_owned(), executor));
    let (items, changes, _) = source(vec![one.clone(), two.clone()]);
    view.tab_items(
        &view.root(),
        "dynamic",
        items,
        selected.bind(Clone::clone, Some(|s, v| *s = v)),
        Rc::new(|s| s.bind(Clone::clone, None)),
        Rc::new(|v, p, _, s| v.label(p, "label", s.bind(Clone::clone, None))),
    );
    assert!(view.clicked_callback("dynamic/two").unwrap().call(()));
    assert_eq!(selected.snapshot(), "two");
    emit(&changes, vec![one]);
    assert_eq!(view.selected("dynamic").as_deref(), Some("one"));
    assert_eq!(selected.snapshot(), "one");
    assert!(view.text("dynamic/two/label").is_none());
}
fn table_fixture() -> Table {
    Table {
        columns: vec![TableColumn {
            title: "Value".into(),
            width: Some(200.0),
            text: TextStyle {
                role: TextRole::Body,
                ..Default::default()
            },
            overflow: TextOverflow::Wrap,
            stretch: false,
            hide_empty: false,
        }]
        .into(),
        rows: vec![Arc::new(TableRow {
            key: "one".into(),
            cells: vec![TableCell {
                text: "value".into(),
                foreground: Some([1, 2, 3]),
            }],
        })]
        .into(),
        ..Default::default()
    }
}
#[test]
fn table_renders_meaningful_cells_selection_and_last_valid_snapshot() {
    let (_, executor, _) = setup();
    let selection = Store::new(None::<TableSelection>, executor);
    let (data, changes, _) = source(table_fixture());
    let view = view();
    view.table_with_selection(
        &view.root(),
        "table",
        data,
        selection.bind(Clone::clone, Some(|s, v| *s = v)),
    );
    assert_eq!(view.text("table/one/0").as_deref(), Some("value"));
    view.click("table/one/0").unwrap();
    assert_eq!(view.table_selection("table").unwrap().row_key, "one");
    let accepted = view.render();
    let mut invalid = table_fixture();
    invalid.rows = vec![invalid.rows[0].clone(), invalid.rows[0].clone()].into();
    emit(&changes, invalid);
    assert_eq!(view.table_snapshot("table"), Some(table_fixture()));
    assert_eq!(accepted, view.render());
    assert!(
        view.select_table(
            "table",
            Some(TableSelection {
                row_key: "missing".into(),
                column: 0
            })
        )
        .is_err()
    );
    let view = View::new(Binding::constant("unsupported".into()));
    let mut invalid = table_fixture();
    Arc::make_mut(&mut invalid.columns)[0].overflow = TextOverflow::EllipsizeMiddle;
    view.table(&view.root(), "table", Binding::constant(invalid));
    assert!(view.table_snapshot("table").is_none());
    assert!(view.error().unwrap().contains("middle ellipsis"));
}
#[derive(Clone, PartialEq, Debug)]
enum Argument {
    Number(u32),
    Toggle(bool),
}
fn menu_item(id: &str, argument: Argument) -> MenuItem<Argument> {
    MenuItem {
        id: id.into(),
        label: id.into(),
        argument,
        shortcut: None,
        role: MenuRole::Normal,
        enabled: true,
        visible: true,
    }
}
#[test]
fn native_hierarchical_menu_preserves_typed_arguments_and_invalid_snapshot_rolls_back() {
    let dispatched = Rc::new(RefCell::new(Vec::new()));
    let out = dispatched.clone();
    let one = menu_item("one", Argument::Number(42));
    let two = menu_item("two", Argument::Toggle(true));
    let (source, changes, _) = source(vec![MenuNode::Submenu {
        id: "sub".into(),
        label: "Sub".into(),
        enabled: true,
        children: vec![MenuNode::Item(one.clone()), MenuNode::Item(two.clone())],
    }]);
    let view = view();
    view.hierarchical_menu(
        "menu",
        Binding::constant("Menu".into()),
        source,
        Rc::new(move |argument| {
            out.borrow_mut().push(argument);
            true
        }),
    );
    let before = view.render();
    assert_eq!(before, view.render());
    view.activate_menu("one").unwrap();
    view.activate_menu("two").unwrap();
    assert_eq!(
        *dispatched.borrow(),
        vec![Argument::Number(42), Argument::Toggle(true)]
    );
    emit(
        &changes,
        vec![MenuNode::Item(one.clone()), MenuNode::Item(one)],
    );
    assert_eq!(before, view.render());
    assert!(view.error().is_some());
    emit(
        &changes,
        vec![MenuNode::Submenu {
            id: "sub".into(),
            label: "Sub".into(),
            enabled: false,
            children: vec![MenuNode::Item(two)],
        }],
    );
    assert!(view.activate_menu("two").is_err());
    assert!(view.activate_menu("one").is_err());
}
#[test]
fn dialog_retains_children_and_footer_invokes_ordered_commands_without_fake_ime_keys() {
    let (_, executor, _) = setup();
    let open = Store::new(false, executor.clone());
    let calls = Store::new(0usize, executor);
    let view = view();
    let dialog = view.dialog(
        &view.root(),
        "dialog",
        Binding::constant("Dialog".into()),
        open.bind(|s| *s, Some(|s, v| *s = v)),
    );
    view.entry(&dialog, "entry", Binding::constant("text".into()));
    let button = FooterButton {
        text: Binding::constant("OK".into()),
        command: calls.command(
            |_| true,
            |s, ()| {
                assert!(s.update(|s| *s += 1));
            },
            Binding::constant(()),
        ),
    };
    view.dialog_footer(
        &dialog,
        "footer",
        DialogFooter {
            accept: button.clone(),
            cancel: button,
            destructive: true,
        },
    );
    let closed = view.render();
    assert!(view.click("footer/accept").is_err());
    view.set_open("dialog", true).unwrap();
    view.click("footer/accept").unwrap();
    assert_eq!(calls.snapshot(), 1);
    assert_ne!(closed, view.render());
    view.set_open("dialog", false).unwrap();
    assert_eq!(view.text("entry").as_deref(), Some("text"));
    assert!(view.edit("entry", "closed").is_err());
}
#[test]
fn host_registered_notifications_are_outside_borrows_and_ownership_is_exclusive() {
    let (_, executor, host) = setup();
    let session = Session::new();
    host.attach(session.clone()).unwrap();
    let notifications = Rc::new(Cell::new(0));
    let count = notifications.clone();
    let weak_host = Rc::downgrade(&host.0);
    host.set_changed(Rc::new(move || {
        count.set(count.get() + 1);
        if let Some(host) = weak_host.upgrade() {
            let _ = Host(host).registered_views();
        }
    }));
    let first = view();
    host.register_view(&session, "first", &first).unwrap();
    assert_eq!(host.registered_views()[0].0, "first");
    let other = Host::new(executor);
    assert!(other.register_view(&session, "foreign", &first).is_err());
    assert!(host.register_view(&session, "alias", &first).is_err());
    first.close();
    assert_eq!(session.window_count(), 0);
    assert!(host.registered_views().is_empty());
    assert!(notifications.get() >= 3);
    let next = view();
    host.register_view(&session, "first", &next).unwrap();
    first.close();
    assert_eq!(session.window_count(), 1);
    host.fail("failure".into());
    assert!(next.is_closed());
    assert!(host.registered_views().is_empty());
    assert_eq!(host.error().as_deref(), Some("failure"));
}
#[test]
fn make_once_partial_failure_and_per_window_confirmation_do_not_close_siblings() {
    let (mut pool, _, host) = setup();
    let session = Session::new();
    let first = view();
    let sibling = view();
    let allowed = Rc::new(Cell::new(false));
    let allow = allowed.clone();
    host.register_view_with_hooks(
        &session,
        "first",
        &first,
        panechord_runtime::WindowHooks {
            confirm: Rc::new(move || {
                let allowed = allow.get();
                Box::pin(async move { allowed })
            }),
            cancel: Rc::new(|| {}),
            drain: Rc::new(|| Box::pin(async {})),
            close: Rc::new(|| {}),
        },
    )
    .unwrap();
    host.register_view(&session, "sibling", &sibling).unwrap();
    first.request_close();
    pool.run_until_stalled();
    assert!(!first.is_closed());
    allowed.set(true);
    first.request_close();
    pool.run_until_stalled();
    assert!(first.is_closed());
    assert!(!sibling.is_closed());
    assert_eq!(session.shutdown().phase(), ShutdownPhase::Running);
    host.close();
    assert!(sibling.is_closed());
    let (_, _, failed) = setup();
    let saved = Rc::new(RefCell::new(None));
    let out = saved.clone();
    assert!(
        failed
            .make(move |_| {
                let partial = view();
                out.replace(Some(partial));
                Err("make failed".into())
            })
            .is_err()
    );
    assert!(saved.borrow().as_ref().unwrap().is_closed());
    assert!(failed.make(|_| panic!("must not execute")).is_err());
}
#[test]
fn native_close_late_install_and_subscription_teardown_are_idempotent() {
    let (title, listener, drops) = source("title".to_owned());
    let view = View::new(title);
    let weak = view.downgrade();
    let late = listener.borrow().as_ref().unwrap().clone();
    let closes = Rc::new(Cell::new(0));
    view.close();
    let count = closes.clone();
    view.set_native_close(Rc::new(move || count.set(count.get() + 1)));
    view.close();
    late("late".into());
    assert_eq!(closes.get(), 1);
    assert_eq!(drops.get(), 1);
    assert!(weak.upgrade().is_none());
    let next = View::new(Binding::constant("native".into()));
    let count = closes.clone();
    next.set_native_close(Rc::new(move || count.set(count.get() + 1)));
    next.native_closed();
    assert_eq!(closes.get(), 1);
}

#[test]
fn stale_combo_snapshot_callback_cannot_select_a_reordered_item() {
    let (_, executor, _) = setup();
    let selected = Store::new(String::new(), executor);
    let first = Choice {
        id: "a".into(),
        label: "A".into(),
    };
    let second = Choice {
        id: "b".into(),
        label: "B".into(),
    };
    let (items, updates, _) = source(vec![first.clone(), second.clone()]);
    let view = view();
    view.combo(
        &view.root(),
        "combo",
        items,
        selected.bind(Clone::clone, Some(|s, v| *s = v)),
    );
    let stale = view.selection_changed_callback("combo").unwrap();
    emit(&updates, vec![second, first]);
    let _ = stale.call(Some(0));
    assert_eq!(selected.snapshot(), "");
    assert!(
        view.selection_changed_callback("combo")
            .unwrap()
            .call(Some(0))
    );
    assert_eq!(selected.snapshot(), "b");
}
#[test]
fn close_releases_scope_keep_even_if_provider_keeps_a_stale_collection_callback() {
    let (_, executor, _) = setup();
    let item = Keyed::new("one", Store::new("one".to_owned(), executor));
    let (items, listener, _) = source(vec![item.clone()]);
    let dropped = Rc::new(Cell::new(0));
    let count = dropped.clone();
    let view = view();
    view.repeat(
        &view.root(),
        "rows",
        items,
        0.0,
        Rc::new(move |v, p, _, s| {
            let count = count.clone();
            v.keep(Subscription::new(move || count.set(count.get() + 1)));
            v.label(p, "label", s.bind(Clone::clone, None));
        }),
    );
    let stale = listener.borrow().as_ref().unwrap().clone();
    view.close();
    assert_eq!(dropped.get(), 1);
    stale(vec![item]);
    assert_eq!(view.node_count(), 0);
    assert_eq!(dropped.get(), 1);
}
#[test]
fn unsupported_menu_selectors_are_errors_and_make_does_not_publish_them() {
    let (_, _, host) = setup();
    let result = host.make(|_| {
        let view = view();
        let mut item = menu_item("copy", Argument::Number(1));
        item.role = MenuRole::Copy;
        view.typed_menu(
            "menu",
            Binding::constant("Edit".into()),
            Binding::constant(vec![item]),
            Rc::new(|_| true),
        );
        Ok(())
    });
    assert!(result.unwrap_err().contains("selectors"));
    assert!(host.is_closed());
    assert!(host.registered_views().is_empty());
}
#[test]
fn old_native_menu_callback_cannot_dispatch_a_replaced_argument() {
    let received = Rc::new(RefCell::new(Vec::new()));
    let out = received.clone();
    let (items, changes, _) = source(vec![menu_item("one", Argument::Number(1))]);
    let view = view();
    view.typed_menu(
        "menu",
        Binding::constant("Menu".into()),
        items,
        Rc::new(move |argument| {
            out.borrow_mut().push(argument);
            true
        }),
    );
    let stale = view.menu_callback("menu").unwrap();
    emit(&changes, vec![menu_item("one", Argument::Number(2))]);
    let _ = stale.call("one".into());
    assert!(received.borrow().is_empty());
    assert!(view.menu_callback("menu").unwrap().call("one".into()));
    assert_eq!(*received.borrow(), vec![Argument::Number(2)]);
}
#[test]
fn failed_make_never_presents_registered_partial_views() {
    let (_, _, host) = setup();
    let session = Session::new();
    let presentations = Rc::new(Cell::new(0));
    let count = presentations.clone();
    assert!(
        host.make(move |host| {
            let partial = view();
            let count = count.clone();
            partial.set_present(Rc::new(move || count.set(count.get() + 1)));
            host.register_view(&session, "partial", &partial)?;
            assert!(host.registered_views().is_empty());
            Err("later failure".into())
        })
        .is_err()
    );
    assert_eq!(presentations.get(), 0);
    assert!(host.registered_views().is_empty());
}

#[test]
fn template_panic_retires_the_entire_projection_and_stale_scopes() {
    let (_, executor, _) = setup();
    let first = Keyed::new("one", Store::new("one".to_owned(), executor.clone()));
    let second = Keyed::new("two", Store::new("two".to_owned(), executor));
    let (items, updates, _) = source(vec![first.clone()]);
    let dropped = Rc::new(Cell::new(0));
    let count = dropped.clone();
    let saved = Rc::new(RefCell::new(Vec::new()));
    let scopes = saved.clone();
    let view = view();
    view.repeat(
        &view.root(),
        "rows",
        items,
        0.0,
        Rc::new(move |v, p, _, model| {
            scopes.borrow_mut().push(v.downgrade());
            let count = count.clone();
            v.keep(Subscription::new(move || count.set(count.get() + 1)));
            v.label(p, "label", model.bind(Clone::clone, None));
            assert_ne!(model.snapshot(), "two", "intentional template failure");
        }),
    );
    emit(&updates, vec![first.clone(), second.clone()]);
    assert_eq!(view.mounted_count("rows"), 0);
    assert!(view.item_keys("rows").is_empty());
    assert_eq!(view.node_count(), 1);
    assert_eq!(dropped.get(), 2);
    assert!(saved.borrow().iter().all(|scope| scope.upgrade().is_none()));
    assert!(view.items_error("rows").unwrap().contains("template"));
    assert!(!first.model.is_closed() && !second.model.is_closed());
}

#[test]
fn oversized_table_is_rejected_without_silent_truncation_or_snapshot_change() {
    let view = view();
    let (table, updates, _) = source(table_fixture());
    view.table(&view.root(), "table", table);
    let before = view.render();
    let mut large = table_fixture();
    large.rows = (0..257)
        .map(|index| {
            Arc::new(TableRow {
                key: format!("row-{index}"),
                cells: vec![TableCell {
                    text: "data".into(),
                    foreground: None,
                }],
            })
        })
        .collect::<Vec<_>>()
        .into();
    emit(&updates, large);
    assert_eq!(view.table_snapshot("table"), Some(table_fixture()));
    assert_eq!(view.render(), before);
    assert!(view.error().unwrap().contains("not truncated"));
}

#[test]
fn published_text_style_limits_preserve_caption_and_table_snapshots() {
    let view = view();
    view.entry(&view.root(), "entry", Binding::constant("readonly".into()));
    let before = view.render();
    assert!(
        view.try_style(
            "entry",
            Style {
                text: Some(TextStyle::default()),
                ..Default::default()
            }
        )
        .unwrap_err()
        .contains("TextBox")
    );
    assert_eq!(before, view.render());
    let (data, updates, _) = source(table_fixture());
    view.table(&view.root(), "table", data);
    let accepted = view.render();
    let stale = view.clicked_callback("table/one/0").unwrap();
    let mut unsupported = table_fixture();
    Arc::make_mut(&mut unsupported.columns)[0].text.role = TextRole::Code;
    emit(&updates, unsupported);
    assert_eq!(view.table_snapshot("table"), Some(table_fixture()));
    assert_eq!(accepted, view.render());
    assert_eq!(stale, view.clicked_callback("table/one/0").unwrap());
    assert!(view.error().unwrap().contains("font_family"));
}

#[test]
fn published_menu_label_mapping_rejects_ambiguity_without_changing_callbacks() {
    let received = Rc::new(RefCell::new(Vec::new()));
    let out = received.clone();
    let mut first = menu_item("one", Argument::Number(1));
    first.label = "Display".into();
    let (items, updates, _) = source(vec![first.clone()]);
    let view = view();
    view.typed_menu(
        "menu",
        Binding::constant("Menu".into()),
        items,
        Rc::new(move |a| {
            out.borrow_mut().push(a);
            true
        }),
    );
    let callback = view.menu_callback("menu").unwrap();
    let before = view.render();
    assert!(callback.call("Display".into()));
    assert_eq!(*received.borrow(), vec![Argument::Number(1)]);
    let mut second = menu_item("two", Argument::Number(2));
    second.label = first.label.clone();
    emit(&updates, vec![first, second]);
    assert_eq!(before, view.render());
    assert_eq!(callback, view.menu_callback("menu").unwrap());
    assert!(view.error().unwrap().contains("ambiguous"));
    let _ = callback.call("one".into());
    assert_eq!(received.borrow().len(), 1);
    view.close();
    let _ = callback.call("Display".into());
    assert_eq!(received.borrow().len(), 1);
}
