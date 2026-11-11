//! Actual Backend::run_host fixture, not a second Reactor application driver.
//! MODEL/STAGE logs are deliberately not READY/native identity evidence.
//! SDK-owned driver readbacks provide separate actual native evidence.
use crate::{Host, View, driver};
use futures::FutureExt;
use panechord_runtime::{
    AutomationView, Backend, Binding, DialogFooter, FooterButton, ImageFrame, Keyed, Layout,
    ListView, MediaView, MenuItem, MenuNode, MenuRole, MenuView, SelectView, Session,
    ShutdownPhase, Store, Style, Table, TableCell, TableColumn, TableRow, Template, TextOverflow,
    TextRole, TextStyle, UiHost, View as FullView, ViewCore, Viewport, WindowHooks,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Smoke,
    MakeError,
    TaskPanic,
    RejectWake,
}

#[derive(Clone)]
struct Model {
    caption: String,
    text: String,
    checked: bool,
    selected: String,
    value: f64,
    page: String,
    nav: String,
    dynamic_page: String,
    dialog: bool,
    visible: bool,
    enabled: bool,
    style: Style,
    image: Option<ImageFrame>,
    menu_count: u32,
}
impl Default for Model {
    fn default() -> Self {
        Self {
            caption: "Panechord workspace".into(),
            text: "Draft note".into(),
            checked: false,
            selected: "draft".into(),
            value: 0.25,
            page: "editor".into(),
            nav: "overview".into(),
            dynamic_page: "one".into(),
            dialog: false,
            visible: true,
            enabled: true,
            style: Style::default(),
            image: Some(pixel([40, 120, 220, 255])),
            menu_count: 0,
        }
    }
}
fn pixel(rgba: [u8; 4]) -> ImageFrame {
    ImageFrame::new(1, 1, Arc::from(rgba)).expect("valid fixture pixel")
}
fn text(value: &str) -> Binding<String> {
    Binding::constant(value.into())
}
fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(format!("smoke: {message}"))
    }
}
async fn turn() {
    futures_timer::Delay::new(Duration::from_millis(600)).await;
}
async fn focus_entry(id: &str, node: &str) -> Result<(), String> {
    driver::native_request_focus(id, node)?;
    // Await the real event, not an assumed synchronous Focus() callback or a
    // fixed settling delay. Each check yields UI dispatch; total wait is bounded.
    for _ in 0..20 {
        futures_timer::Delay::new(Duration::from_millis(25)).await;
        if driver::native_focus_delivered(id, node)? {
            return Ok(());
        }
    }
    Err(format!("smoke: GotFocus delivery deadline: {id}/{node}"))
}

/// Actual native facts for a shared-YAML diagnostic, not retained View text.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct NativeTextProbe {
    pub generation: u64,
    pub hwnd: usize,
    pub entry_token: u64,
    pub label_token: u64,
    pub entry_text: String,
    pub label_text: String,
    pub text_events: u64,
}

/// Resolve the actual owning Host for teardown diagnostics through a facade View.
#[doc(hidden)]
pub fn view_host(view: &View) -> Result<Host, String> {
    view.0
        .host
        .borrow()
        .as_ref()
        .and_then(std::rc::Weak::upgrade)
        .map(Host)
        .ok_or_else(|| "diagnostic View has no live owning Host".into())
}

/// UI-local Loaded/XamlRoot and native text readback. `None` means not mounted
/// or Loaded yet. Optional input calls the actual TextBox.SetText, never edit()
/// or a simulated callback. Native faults use the driver's deferred fault path.
#[doc(hidden)]
pub fn native_text_probe(
    view: &View,
    entry: &str,
    label: &str,
    set_text: Option<&str>,
) -> Result<Option<NativeTextProbe>, String> {
    driver::diagnostic_text_probe(view, entry, label, set_text)
}

/// Builds all twelve shared capabilities using the generic View contract.
/// IME/OS input and automatic scrolling are not approximated or claimed.
fn controls<V: FullView>(
    view: &V,
    model: &Store<Model>,
    rows: &Store<Vec<Keyed<String>>>,
) -> Result<(), String> {
    let root = view.root();
    let shell = view.try_layout(
        &root,
        "window-body",
        Layout::Stack {
            horizontal: false,
            gap: 5.0,
        },
    )?;
    view.label(&shell, "pinned-header", text("Workspace"));
    let scroll = view.try_scroll(&shell, "scroll", false)?;
    view.label(&shell, "pinned-footer", text("Ready"));
    let body = view.try_layout(
        &scroll,
        "body",
        Layout::Stack {
            horizontal: false,
            gap: 8.0,
        },
    )?;
    view.style(
        "body",
        Style {
            padding: panechord_runtime::Edges {
                top: 12.0,
                right: 12.0,
                bottom: 12.0,
                left: 12.0,
            },
            ..Default::default()
        },
    );
    view.label(&body, "heading", text("Project notes"));
    view.style(
        "heading",
        Style {
            text: Some(TextStyle {
                role: TextRole::Heading,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    view.label(&body, "echo", model.bind(|s| s.text.clone(), None));
    view.entry_with_placeholder(
        &body,
        "entry",
        model.bind(|s| s.text.clone(), Some(|s, v| s.text = v)),
        text("Write a short note"),
    );
    view.entry(&body, "readonly", text("Read-only reference"));
    let command = model.command(
        |s| s.enabled,
        |store, ()| {
            assert!(store.update(|s| s.text.push('!')), "accepted note update");
        },
        Binding::constant(()),
    );
    view.button(&body, "save", text("Save draft"), command.clone());
    view.visible("save", model.bind(|s| s.visible, None));
    view.enabled("save", model.bind(|s| s.enabled, None));
    view.style_binding("echo", model.bind(|s| s.style, None));
    let flex = view.try_layout(&body, "flex", Layout::Flex { gap: 7.0 })?;
    view.style_binding(
        "flex",
        model.bind(
            |s| Style {
                placement: panechord_runtime::Placement {
                    width: Some(if s.style.placement.width.is_some() {
                        288.0
                    } else {
                        188.0
                    }),
                    ..Default::default()
                },
                padding: panechord_runtime::Edges {
                    left: 11.0,
                    top: 13.0,
                    right: 17.0,
                    bottom: 19.0,
                },
                horizontal_alignment: Some(panechord_runtime::Alignment::Start),
                ..Default::default()
            },
            None,
        ),
    );
    for (id, width, height) in [
        ("flex-a", 60.0, 20.0),
        ("flex-b", 80.0, 30.0),
        ("flex-c", 25.0, 10.0),
    ] {
        view.label(&flex, id, text(""));
        view.style(
            id,
            Style {
                placement: panechord_runtime::Placement {
                    width: Some(width),
                    height: Some(height),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
    }
    view.style(
        "flex-a",
        Style {
            placement: panechord_runtime::Placement {
                width: Some(60.0),
                height: Some(20.0),
                ..Default::default()
            },
            margin: panechord_runtime::Edges {
                left: 2.0,
                top: 1.0,
                right: 3.0,
                bottom: 4.0,
            },
            padding: panechord_runtime::Edges {
                left: 3.0,
                right: 5.0,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    view.style_binding(
        "flex-b",
        model.bind(
            |s| Style {
                placement: panechord_runtime::Placement {
                    width: Some(if s.style.placement.width.is_some() {
                        120.0
                    } else {
                        80.0
                    }),
                    height: Some(30.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            None,
        ),
    );
    view.visible("flex-c", model.bind(|s| s.visible, None));
    view.separator(&body, "separator", false);
    let grid = view.try_layout(
        &body,
        "selectors",
        Layout::Grid {
            columns: 2,
            gap: 8.0,
        },
    )?;
    let span_grid = view.try_layout(
        &body,
        "placement-grid",
        Layout::Grid {
            columns: 2,
            gap: 4.0,
        },
    )?;
    view.label(&span_grid, "spanned-label", text("Grid span"));
    view.style(
        "spanned-label",
        Style {
            placement: panechord_runtime::Placement {
                row: 1,
                column: 0,
                row_span: 2,
                column_span: 2,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    view.checkbox(
        &grid,
        "check",
        text("Reviewed"),
        model.bind(|s| s.checked, Some(|s, v| s.checked = v)),
    );
    let selected = model.bind(|s| s.selected.clone(), Some(|s, v| s.selected = v));
    view.radio(&grid, "draft", text("Draft"), selected.clone(), "draft");
    view.radio(
        &grid,
        "published",
        text("Published"),
        selected.clone(),
        "published",
    );
    view.combo(
        &grid,
        "combo",
        Binding::constant(vec![
            panechord_runtime::Choice {
                id: "draft".into(),
                label: "Draft".into(),
            },
            panechord_runtime::Choice {
                id: "published".into(),
                label: "Published".into(),
            },
        ]),
        selected,
    );
    let value = model.bind(|s| s.value, Some(|s, v| s.value = v));
    view.slider(
        &body,
        "slider",
        value.clone(),
        panechord_runtime::SliderRange {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    );
    view.meter(&body, "meter", value);
    let canvas = view.try_layout(&body, "canvas", Layout::Absolute)?;
    view.style(
        "canvas",
        Style {
            placement: panechord_runtime::Placement {
                height: Some(24.0),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    view.label(&canvas, "canvas-label", text("Canvas"));
    view.style(
        "canvas-label",
        Style {
            placement: panechord_runtime::Placement {
                x: 12.0,
                y: 4.0,
                width: Some(120.0),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let image = model.bind(|s| s.image.clone(), Some(|s, v| s.image = v));
    view.image(&body, "thumbnail", image.clone());
    view.image_with_pointer(&body, "preview", image, Binding::constant(None));
    view.style(
        "preview",
        Style {
            placement: panechord_runtime::Placement {
                width: Some(32.0),
                height: Some(32.0),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let tabs = view.tab(
        &body,
        "tabs",
        model.bind(|s| s.page.clone(), Some(|s, v| s.page = v)),
    );
    let page = view.try_tab_page(&tabs, "editor-page", text("Editor"), "editor")?;
    view.label(&page, "editor-help", text("Edit and save a draft."));
    let page = view.tab_page(&tabs, "history-page", text("History"), "history");
    view.label(&page, "history-help", text("No previous revisions."));
    view.tab_position("tabs", panechord_runtime::TabPosition::Top)?;
    view.finish_tab(&tabs)?;
    let navigation = view.navigation(
        &body,
        "navigation",
        model.bind(|s| s.nav.clone(), Some(|s, v| s.nav = v)),
    );
    for key in ["overview", "details"] {
        let page = view.tab_page(&navigation, &format!("nav-{key}"), text(key), key);
        view.label(
            &page,
            &format!("nav-text-{key}"),
            text("Workspace navigation"),
        );
    }
    view.finish_tab(&navigation)?;
    let template: Template<V, V::Parent, String> = Rc::new(|v, p, _, row| {
        v.label(p, "name", row.bind(Clone::clone, None));
        v.entry(p, "edit", row.bind(Clone::clone, Some(|s, v| *s = v)));
    });
    let source = rows.bind(Clone::clone, Some(|s, v| *s = v));
    view.repeat(&body, "repeat", source.clone(), 4.0, template.clone());
    view.items(
        &body,
        "items",
        source.clone(),
        Binding::constant(Viewport {
            start: 0,
            count: 1,
            row_height: 24.0,
        }),
        template.clone(),
    );
    view.content(
        &body,
        "content",
        source.map(|rows| rows.first().cloned()),
        template.clone(),
    );
    view.tab_items(
        &body,
        "dynamic-tabs",
        source,
        model.bind(|s| s.dynamic_page.clone(), Some(|s, v| s.dynamic_page = v)),
        Rc::new(|row| row.bind(Clone::clone, None)),
        template,
    );
    let table = Table {
        columns: vec![TableColumn {
            title: "Note".into(),
            width: None,
            text: TextStyle::default(),
            overflow: TextOverflow::Wrap,
            stretch: true,
            hide_empty: false,
        }]
        .into(),
        rows: vec![Arc::new(TableRow {
            key: "draft".into(),
            cells: vec![TableCell {
                text: "Project note".into(),
                foreground: None,
            }],
        })]
        .into(),
        empty_text: "No notes".into(),
        column_gap: 8.0,
        row_gap: 4.0,
        row_inset: 2.0,
    };
    view.table(&body, "table-readonly", Binding::constant(table.clone()));
    view.table_with_selection(
        &body,
        "table",
        Binding::constant(table),
        Binding::constant(None),
    );
    let dialog = view.dialog(
        &body,
        "dialog",
        text("Save changes?"),
        model.bind(|s| s.dialog, Some(|s, v| s.dialog = v)),
    );
    view.label(
        &dialog,
        "dialog-text",
        text("Keep this note in the workspace."),
    );
    let dismiss = model.command(
        |_| true,
        |s, ()| {
            assert!(s.update(|s| s.dialog = false));
        },
        Binding::constant(()),
    );
    view.dialog_footer(
        &dialog,
        "footer",
        DialogFooter {
            accept: FooterButton {
                text: text("Save"),
                command: dismiss.clone(),
            },
            cancel: FooterButton {
                text: text("Cancel"),
                command: dismiss,
            },
            destructive: false,
        },
    );
    let item = |id: &str, shortcut: &str| MenuItem {
        id: id.into(),
        label: "Save note".into(),
        argument: (),
        enabled: true,
        visible: true,
        role: MenuRole::Normal,
        shortcut: Some(shortcut.into()),
    };
    let dispatch = {
        let model = model.clone();
        Rc::new(move |()| model.update(|s| s.menu_count += 1))
    };
    view.typed_menu(
        "file-menu",
        text("File"),
        model.bind(
            |s| {
                vec![
                    MenuItem {
                        id: "file-save".into(),
                        label: "Save note".into(),
                        argument: (),
                        enabled: s.enabled,
                        visible: s.visible,
                        role: MenuRole::Normal,
                        shortcut: Some("Primary+S".into()),
                    },
                    MenuItem {
                        id: "file-quit".into(),
                        label: "Quit".into(),
                        argument: (),
                        enabled: true,
                        visible: true,
                        role: MenuRole::Quit,
                        shortcut: Some("Primary+Q".into()),
                    },
                ]
            },
            None,
        ),
        dispatch.clone(),
    );
    view.hierarchical_menu(
        "tools-menu",
        model.bind(|s| if s.visible { "Tools" } else { "More" }.into(), None),
        Binding::constant(vec![MenuNode::Submenu {
            id: "notes-submenu".into(),
            label: "Notes".into(),
            enabled: true,
            children: vec![MenuNode::Item(item("tools-save", "Primary+Shift+S"))],
        }]),
        dispatch,
    );
    view.visible("tools-menu", model.bind(|s| s.visible, None));
    view.typed_menu(
        "edit-menu",
        text("Edit"),
        Binding::constant(
            [
                ("edit-copy", "Copy", MenuRole::Copy),
                ("edit-cut", "Cut", MenuRole::Cut),
                ("edit-paste", "Paste", MenuRole::Paste),
                ("edit-select", "Select All", MenuRole::SelectAll),
            ]
            .into_iter()
            .map(|(id, label, role)| MenuItem {
                id: id.into(),
                label: label.into(),
                role,
                argument: (),
                shortcut: None,
                enabled: true,
                visible: true,
            })
            .collect(),
        ),
        Rc::new(|()| panic!("edit selectors must not dispatch the application argument")),
    );
    view.error().map_or(Ok(()), Err)
}

struct Fixture {
    id: String,
    view: View,
    model: Store<Model>,
    rows: Store<Vec<Keyed<String>>>,
    allow: Rc<Cell<bool>>,
    confirmations: Rc<Cell<u32>>,
    cancelled: Rc<Cell<u32>>,
    closed: Rc<Cell<u32>>,
}
fn window(host: &Host, session: &Session, id: &str) -> Result<Fixture, String> {
    let model = Store::new(Model::default(), host.executor());
    let rows = Store::new(
        vec![
            Keyed::new("one", Store::new("First note".to_owned(), host.executor())),
            Keyed::new("two", Store::new("Second note".to_owned(), host.executor())),
        ],
        host.executor(),
    );
    let view = host.new_view(model.bind(|s| s.caption.clone(), Some(|s, v| s.caption = v)))?;
    controls(&view, &model, &rows)?;
    let allow = Rc::new(Cell::new(false));
    let confirmations = Rc::new(Cell::new(0));
    let cancelled = Rc::new(Cell::new(0));
    let closed = Rc::new(Cell::new(0));
    let hooks = WindowHooks {
        confirm: {
            let allow = allow.clone();
            let confirmations = confirmations.clone();
            Rc::new(move || {
                let allow = allow.clone();
                let confirmations = confirmations.clone();
                async move {
                    futures_timer::Delay::new(Duration::from_millis(30)).await;
                    confirmations.set(confirmations.get() + 1);
                    allow.get()
                }
                .boxed_local()
            })
        },
        cancel: {
            let count = cancelled.clone();
            let model = model.clone();
            let rows = rows.clone();
            Rc::new(move || {
                count.set(count.get() + 1);
                model.close();
                rows.close();
            })
        },
        drain: Rc::new(|| {
            async {
                futures_timer::Delay::new(Duration::from_millis(20)).await;
            }
            .boxed_local()
        }),
        close: {
            let closed = closed.clone();
            let weak = view.downgrade();
            Rc::new(move || {
                closed.set(closed.get() + 1);
                if let Some(view) = weak.upgrade() {
                    view.close();
                }
            })
        },
    };
    host.register_view_with_hooks(session, id, &view, hooks)?;
    Ok(Fixture {
        id: id.into(),
        view,
        model,
        rows,
        allow,
        confirmations,
        cancelled,
        closed,
    })
}

async fn veto_then_close(
    target: &Fixture,
    sibling: &Fixture,
    session: &Session,
) -> Result<(), String> {
    let before = target.confirmations.get();
    let identity = driver::native_title(&target.id)?;
    driver::request_native_close(&target.id)?;
    turn().await;
    let preserved = driver::native_title(&target.id)?;
    require(
        identity.0 == preserved.0 && identity.1 == preserved.1,
        "WM_CLOSE veto replaced native identity",
    )?;
    require(
        target.confirmations.get() == before + 1 && !target.view.is_closed(),
        "per-window veto did not preserve target",
    )?;
    require(
        target.cancelled.get() == 0 && target.closed.get() == 0,
        "veto ran teardown hooks",
    )?;
    require(
        !sibling.view.is_closed() && session.shutdown().phase() == ShutdownPhase::Running,
        "veto affected sibling/session",
    )?;
    sibling.view.present();
    turn().await;
    target.allow.set(true);
    driver::request_native_close(&target.id)?;
    turn().await;
    require(
        target.view.is_closed() && target.cancelled.get() == 1 && target.closed.get() == 1,
        "approved close hooks must run once",
    )?;
    require(
        target.model.is_closed() && target.rows.is_closed() && !sibling.view.is_closed(),
        "approved close leaked model or closed sibling",
    )?;
    Ok(())
}

async fn scenario(
    host: Rc<Host>,
    session: Session,
    first: Fixture,
    passed: Rc<Cell<bool>>,
) -> Result<(), String> {
    turn().await;
    let sibling = window(&host, &session, "sibling")?;
    turn().await;
    require(
        first.view.title() == sibling.view.title(),
        "initial captions must match",
    )?;
    let native_first = driver::native_title("main")?;
    let native_sibling = driver::native_title("sibling")?;
    require(
        native_first.2 == native_sibling.2
            && native_first.2 == first.view.title()
            && native_first.1 != native_sibling.1,
        "same-caption native readback/identity failed",
    )?;
    println!("STAGE same-caption windows=2");
    require(
        first.model.update(|s| s.caption.clear()) && sibling.model.update(|s| s.caption.clear()),
        "title update rejected",
    )?;
    turn().await;
    require(
        first.view.title().is_empty() && sibling.view.title().is_empty(),
        "reactive empty titles failed",
    )?;
    let updated_first = driver::native_title("main")?;
    let updated_sibling = driver::native_title("sibling")?;
    require(
        updated_first.2.is_empty()
            && updated_sibling.2.is_empty()
            && updated_first.1 == native_first.1
            && updated_sibling.1 == native_sibling.1,
        "empty native titles or stable HWND failed",
    )?;
    println!("NATIVE_TITLE_PASS same-caption empty-caption stable-hwnd=true");
    println!("STAGE empty-title windows=2");
    // Opening sibling activated its Window last. Native editor operations need
    // an active owner, not merely Focus() on an inactive XAML tree. Present is
    // queued by the driver; yield a real UI turn before checking GotFocus.
    println!("STAGE editor-owner-present id=main");
    first.view.present();
    turn().await;
    let root_identity = driver::native_identity("main", "__root")?;
    let menu_identity = driver::native_identity("main", "tools-menu")?;
    let original_height = driver::native_window_layout("main", false)?;
    driver::native_scroll("main", true)?;
    turn().await;
    driver::native_window_layout("main", true)?;
    driver::native_scroll("main", false)?;
    let original_size = driver::native_resize("main", 720, 640)?;
    turn().await;
    let smaller_height = driver::native_window_layout("main", false)?;
    require(
        smaller_height < original_height - 50.0,
        "resize did not reduce native viewport",
    )?;
    driver::native_resize("main", original_size.0, original_size.1)?;
    turn().await;
    // The window manager may clamp the requested outer size to its work area.
    // The driver separately compares XAML's root with the actual HWND client.
    require(
        driver::native_window_layout("main", false)? > smaller_height + 50.0,
        "enlarging did not increase native viewport",
    )?;
    require(
        driver::native_identity("main", "__root")? == root_identity,
        "resize replaced window root",
    )?;
    // Automation and callbacks exercise retained state; native publication is
    // separately proved by the driver logs, not by these model observations.
    driver::native_assertions("main")?;
    for editor in ["entry", "readonly", "entry"] {
        focus_entry("main", editor).await?;
        driver::native_menu_edit_assertions("main", editor)?;
    }
    // Guest-only native smoke scenario. No host tests/app execution is needed
    // to compile this; the Sandbox's local clipboard needs no host redirection.
    let original = first.model.snapshot().text;
    first.view.activate_menu("edit-select")?;
    first.view.activate_menu("edit-copy")?;
    first.view.activate_menu("edit-cut")?;
    turn().await;
    require(
        first.model.snapshot().text.is_empty(),
        "native Cut did not update Store",
    )?;
    first.view.activate_menu("edit-paste")?;
    turn().await;
    require(
        first.model.snapshot().text == original,
        "native Paste did not restore clipboard text",
    )?;
    println!(
        "NATIVE_MENU_CLIPBOARD_PASS Copy-Cut-Paste-native=true Store-roundtrip=true no-physical-input-claim"
    );
    let retained_one = driver::native_identity("main", "repeat/one")?;
    let retained_two = driver::native_identity("main", "repeat/two")?;
    driver::native_set_text("main", "entry", "Edited note")?;
    turn().await;
    require(
        first.model.snapshot().text == "Edited note",
        "real native TextChanged did not reach Store",
    )?;
    driver::native_assertions("main")?;
    println!(
        "NATIVE_EVENT_ROUNDTRIP_PASS TextBox.SetText->TextChanged->Store->TextBlock no-OS-input-claim"
    );
    require(
        first
            .view
            .clicked_callback("save")
            .ok_or("button callback missing")?
            .call(()),
        "button callback rejected",
    )?;
    driver::native_selectors("main")?;
    first.view.select("tabs", "history")?;
    first.view.select("navigation", "details")?;
    first.view.activate_menu("file-save")?;
    first.view.activate_menu("tools-save")?;
    first.view.viewport("items", 1);
    require(
        first.model.update(|s| {
            s.image = Some(pixel([220, 120, 40, 255]));
            s.style = Style {
                placement: panechord_runtime::Placement {
                    width: Some(400.0),
                    height: Some(48.0),
                    ..Default::default()
                },
                margin: panechord_runtime::Edges {
                    left: 3.0,
                    top: 4.0,
                    right: 5.0,
                    bottom: 6.0,
                },
                padding: panechord_runtime::Edges {
                    left: 7.0,
                    ..Default::default()
                },
                horizontal_alignment: Some(panechord_runtime::Alignment::Center),
                vertical_alignment: Some(panechord_runtime::Alignment::End),
                text: Some(TextStyle {
                    role: TextRole::Subtitle,
                    foreground: Some([25, 75, 125]),
                    ..Default::default()
                }),
            };
        }),
        "reactive media/style update rejected",
    )?;
    turn().await;
    require(
        first.view.text("echo").as_deref() == Some("Edited note!")
            && first.model.snapshot().checked,
        "entry/button/checkbox callback roundtrip failed",
    )?;
    require(
        first.view.slider_value("slider") == Some(0.75)
            && first.model.snapshot().selected == "published",
        "selectors failed",
    )?;
    require(
        first.view.mounted_count("items") == 1
            && first.view.item_keys("repeat").len() == 2
            && first.view.items_error("items").is_none(),
        "collection probes failed",
    )?;
    require(
        first.model.snapshot().menu_count == 2 && first.view.image_update_count() >= 4,
        "menu/media updates failed",
    )?;
    require(
        first.rows.update(|rows| rows.swap(0, 1)),
        "reactive collection reorder rejected",
    )?;
    turn().await;
    require(
        first.view.item_keys("repeat") == ["two", "one"]
            && first.view.mounted_count("content") == 1,
        "reactive repeat/content reconciliation failed",
    )?;
    require(
        driver::native_identity("main", "repeat/one")? == retained_one
            && driver::native_identity("main", "repeat/two")? == retained_two,
        "keyed reorder replaced native node identity",
    )?;
    driver::native_assertions("main")?;
    println!("NATIVE_KEY_IDENTITY_PASS retained-keyed-reorder=true");
    let obsolete = first
        .rows
        .snapshot()
        .into_iter()
        .find(|row| row.key == "one")
        .ok_or("old row missing")?
        .model;
    let obsolete_text = obsolete.snapshot();
    focus_entry("main", "repeat/one/edit").await?;
    let old_event = driver::native_hold_entry("main", "repeat/one/edit")?;
    let new_row = Store::new("Replacement note".to_owned(), host.executor());
    let next_row = new_row.clone();
    require(
        first.rows.update(move |rows| {
            let row = rows
                .iter_mut()
                .find(|row| row.key == "one")
                .expect("row exists");
            row.model = next_row;
        }),
        "row identity replacement rejected",
    )?;
    turn().await;
    require(
        driver::native_identity("main", "repeat/one")? != retained_one,
        "same logical ID kept replaced native object",
    )?;
    driver::native_removed_token("main", old_event)?;
    require(
        obsolete.snapshot() == obsolete_text && new_row.snapshot() == "Replacement note",
        "removed native generation changed model",
    )?;
    require(
        first.model.update(|s| s.style = Style::default()),
        "style clear rejected",
    )?;
    turn().await;
    driver::native_assertions("main")?;
    println!("NATIVE_STYLE_CLEAR_PASS auto-NaN margin-padding foreground font-default");
    require(
        first.view.edit("readonly", "not accepted").is_err(),
        "readonly entry accepted edits",
    )?;
    require(
        first.model.update(|s| {
            s.visible = false;
            s.enabled = false;
        }),
        "applicability update rejected",
    )?;
    turn().await;
    require(
        first.view.click("save").is_err(),
        "hidden/disabled button accepted invocation",
    )?;
    driver::native_assertions("main")?;
    driver::native_scroll("main", false)?;
    turn().await;
    driver::native_window_layout("main", false)?;
    require(
        first.model.update(|s| {
            s.visible = true;
            s.enabled = true;
        }),
        "applicability restore rejected",
    )?;
    turn().await;
    driver::native_scroll("main", false)?;
    turn().await;
    driver::native_window_layout("main", false)?;
    require(
        driver::native_identity("main", "tools-menu")? == menu_identity,
        "menu update replaced MenuBarItem",
    )?;
    first.view.set_open("dialog", true)?;
    turn().await;
    driver::native_dialog("main", true)?;
    first.view.set_open("dialog", false)?;
    turn().await;
    driver::native_dialog("main", false)?;
    require(
        first.view.error().is_none(),
        "native publication/control error",
    )?;
    println!("STAGE all-controls roundtrip=pass");
    let confirmations = first.confirmations.get();
    first.view.activate_menu("file-quit")?;
    turn().await;
    require(
        !first.view.is_closed()
            && first.confirmations.get() == confirmations + 1
            && !sibling.view.is_closed(),
        "menu Quit bypassed veto or affected sibling",
    )?;
    veto_then_close(&first, &sibling, &session).await?;
    println!("STAGE main-closed sibling-present");
    let replacement = window(&host, &session, "main")?;
    require(
        !replacement.view.same_view(&first.view) && first.view.downgrade().upgrade().is_none(),
        "reused ID preserved old View identity",
    )?;
    turn().await;
    println!("STAGE id-reuse id=main");
    veto_then_close(&sibling, &replacement, &session).await?;
    let auxiliary = window(&host, &session, "auxiliary")?;
    turn().await;
    replacement.allow.set(true);
    replacement.view.request_close();
    turn().await;
    require(
        replacement.view.is_closed() && session.window_count() == 1 && !host.is_closed(),
        "auxiliary did not keep application alive",
    )?;
    auxiliary.view.present();
    require(
        auxiliary
            .model
            .update(|s| s.text = "Auxiliary continues".into()),
        "auxiliary work rejected",
    )?;
    turn().await;
    require(
        auxiliary.view.text("echo").as_deref() == Some("Auxiliary continues"),
        "auxiliary continuation failed",
    )?;
    println!("STAGE auxiliary-continue windows=1");
    auxiliary.allow.set(true);
    passed.set(true);
    println!("STAGE last-close-request autoexit-required");
    auxiliary.view.request_close();
    // No host.close()/Session Quit here: only last-window native close may exit.
    Ok(())
}

/// Runs the same full fixture for --smoke and --p1-smoke.
struct TaskDrop(Rc<Cell<bool>>);
impl Drop for TaskDrop {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

pub fn run<B: Backend<View = View, Host = Host>>(mode: Mode) -> Result<(), String> {
    B::set_initial_window_size(800, 900);
    let passed = Rc::new(Cell::new(false));
    let completed = passed.clone();
    let captured = Rc::new(RefCell::new(None::<(Rc<Host>, Session)>));
    let capture = captured.clone();
    let task_dropped = Rc::new(Cell::new(false));
    let dropped = task_dropped.clone();
    let result = B::run_host(move |host| {
        let session = Session::new();
        host.attach(session.clone())?;
        capture.replace(Some((host.clone(), session.clone())));
        // Construct outside the async block, so even a never-polled accepted
        // task owns its teardown guard. Check after Backend::run_host returns.
        let guard = TaskDrop(dropped);
        host.executor().spawn(
            async move {
                let _guard = guard;
                futures::future::pending::<()>().await;
            }
            .boxed_local(),
        );
        let first = window(&host, &session, "main")?;
        if mode == Mode::MakeError {
            return Err("intentional make error".into());
        }
        let deadline = host.clone();
        host.executor().spawn(
            async move {
                futures_timer::Delay::new(Duration::from_secs(24)).await;
                deadline
                    .fail("smoke watchdog: last close did not autoexit or scenario stalled".into());
            }
            .boxed_local(),
        );
        match mode {
            Mode::Smoke => {
                let owner = host.clone();
                host.executor().spawn(
                    async move {
                        if let Err(error) = scenario(owner.clone(), session, first, completed).await
                        {
                            owner.fail(error);
                        }
                    }
                    .boxed_local(),
                );
            }
            Mode::TaskPanic => host.executor().spawn(
                async move {
                    turn().await;
                    panic!("intentional task panic");
                }
                .boxed_local(),
            ),
            Mode::RejectWake => {
                let owner = host.clone();
                host.executor().spawn(
                    async move {
                        turn().await;
                        if let Err(error) = driver::inject_rejected_wake() {
                            owner.fail(error);
                        }
                    }
                    .boxed_local(),
                );
            }
            Mode::MakeError => unreachable!(),
        }
        Ok(())
    });
    let (host, session) = captured
        .borrow()
        .clone()
        .ok_or("Backend never called make")?;
    require(
        host.is_closed() && host.registered_views().is_empty() && session.window_count() == 0,
        "host/session teardown incomplete",
    )?;
    require(
        task_dropped.get(),
        "accepted auxiliary task was not dropped on Backend exit",
    )?;
    if mode == Mode::Smoke {
        result?;
        require(passed.get(), "Backend exited before full scenario")?;
        println!("P1 SMOKE PASS generic-backend dynamic-windows all-controls last-close-autoexit");
        Ok(())
    } else {
        let expected = match mode {
            Mode::MakeError => "intentional make error",
            Mode::TaskPanic => "local task panicked",
            Mode::RejectWake => "PostMessageW rejected wake",
            Mode::Smoke => unreachable!(),
        };
        let error = result
            .err()
            .ok_or("fault injection unexpectedly succeeded")?;
        require(error.contains(expected), "wrong fault error")?;
        println!(
            "FAULT_HOST_TEARDOWN_PASS host-closed registrations=0 session-windows=0 accepted-task-dropped=true"
        );
        // Native/task teardown evidence must be added by driver diagnostics;
        // do not print P0's stronger FAULT_TEARDOWN_PASS from model checks.
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct NoTasks;
    impl panechord_runtime::Executor for NoTasks {
        fn spawn(&self, _: futures::future::LocalBoxFuture<'static, ()>) {
            panic!("unexpected task in retained controls test");
        }
    }
    #[test]
    fn generic_full_capability_controls_mount_and_roundtrip() {
        let executor = Rc::new(NoTasks);
        let model = Store::new(Model::default(), executor.clone());
        let rows = Store::new(
            vec![Keyed::new(
                "one",
                Store::new("Note".to_owned(), executor.clone()),
            )],
            executor,
        );
        let view = View::new(text("same caption"));
        controls(&view, &model, &rows).unwrap();
        assert!(view.node_count() > 30);
        view.edit("entry", "Updated").unwrap();
        view.click("save").unwrap();
        assert_eq!(view.text("echo").as_deref(), Some("Updated!"));
        view.toggle("check", true).unwrap();
        view.select("combo", "published").unwrap();
        assert!(model.snapshot().checked);
        assert_eq!(model.snapshot().selected, "published");
        assert!(view.edit("readonly", "reject").is_err());
        assert_eq!(view.item_keys("repeat"), ["one"]);
        assert_eq!(view.mounted_count("items"), 1);
        assert!(view.error().is_none());
        view.close();
        assert!(view.downgrade().upgrade().is_none());
    }
}
