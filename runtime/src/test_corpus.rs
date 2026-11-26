//! Shared full-corpus builder for the exact layout goldens.
//!
//! Test-only helper (`test-corpus` feature): never compiled into release
//! builds. Both backends build byte-identical view trees from this one
//! function, so construction parity is enforced by construction instead of
//! review. Geometry still lives in per-backend goldens (`*_tests/golden/`
//! next to each backend) because native widgets differ by design; the
//! cross-backend-identical subset is covered by the shared minimal golden.

use super::{
    Binding, Choice, DialogFooter, Executor, FooterButton, ImageFrame, Keyed, Layout, Placement,
    SliderRange, Store, Style, Table, TableCell, TableColumn, TableRow, TextOverflow, TextRole,
    TextStyle, View, Viewport,
};
use std::{rc::Rc, sync::Arc};

/// One list row of the full corpus.
#[derive(Clone, Default)]
pub struct FullRow {
    pub title: String,
    pub note: String,
    pub flag: bool,
}

/// The full corpus model: every control and layout kind plus nesting.
#[derive(Clone, Default)]
pub struct FullModel {
    pub entry_text: String,
    pub flag: bool,
    pub picked: String,
    pub level: f64,
    pub combo_items: Vec<Choice>,
    pub combo_sel: String,
    pub progress: f64,
    pub picture: Option<ImageFrame>,
    pub rows: Vec<Keyed<FullRow>>,
    pub detail: Option<Keyed<String>>,
    pub table: Table,
}

/// Retained handles for a built corpus; hold until the view closes.
pub struct FullCorpus {
    pub model: Store<FullModel>,
}

/// Monospace leaf style with an explicit footprint, shared by both backends
/// so natural font metrics cannot leak into geometry.
pub fn code_style(width: Option<f32>, height: Option<f32>) -> Style {
    Style {
        placement: Placement {
            width,
            height,
            ..Default::default()
        },
        text: Some(TextStyle {
            role: TextRole::Code,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Build the full corpus under `view.root()`. Window sizing, presentation
/// and golden comparison stay at the call site (backend-owned hosts).
pub fn build_full_corpus<V: View>(view: &V, executor: &Rc<dyn Executor>) -> FullCorpus {
    const H: f32 = 36.0;
    let picture = ImageFrame::new(1, 1, Arc::new([255u8, 0, 0, 255])).unwrap();
    let rows: Vec<Keyed<FullRow>> = (0..3)
        .map(|index| {
            Keyed::new(
                format!("r{index}"),
                Store::new(
                    FullRow {
                        title: format!("row {index}"),
                        note: format!("note {index}"),
                        flag: index % 2 == 0,
                    },
                    executor.clone(),
                ),
            )
        })
        .collect();
    let model = Store::new(
        FullModel {
            entry_text: "edit me".into(),
            flag: true,
            picked: "a".into(),
            level: 42.0,
            combo_items: ["a", "b"]
                .into_iter()
                .map(|id| Choice {
                    id: id.into(),
                    label: format!("Choice {id}"),
                })
                .collect(),
            combo_sel: "a".into(),
            progress: 0.5,
            picture: Some(picture),
            rows,
            detail: Some(Keyed::new(
                "c0".to_owned(),
                Store::new("Shown".to_owned(), executor.clone()),
            )),
            table: Table {
                columns: vec![
                    TableColumn {
                        title: "K".into(),
                        width: Some(120.0),
                        text: TextStyle {
                            role: TextRole::Code,
                            ..Default::default()
                        },
                        overflow: TextOverflow::Clip,
                        stretch: false,
                        hide_empty: false,
                    },
                    TableColumn {
                        title: "V".into(),
                        width: Some(220.0),
                        text: TextStyle {
                            role: TextRole::Code,
                            ..Default::default()
                        },
                        overflow: TextOverflow::Clip,
                        stretch: false,
                        hide_empty: false,
                    },
                ]
                .into(),
                rows: (0..2)
                    .map(|index| {
                        Arc::new(TableRow {
                            key: format!("t{index}"),
                            cells: vec![
                                TableCell {
                                    text: format!("k{index}"),
                                    foreground: None,
                                },
                                TableCell {
                                    text: format!("v{index}"),
                                    foreground: None,
                                },
                            ],
                        })
                    })
                    .collect::<Vec<_>>()
                    .into(),
                column_gap: 8.0,
                row_gap: 4.0,
                row_inset: 0.0,
                empty_text: "empty".into(),
            },
        },
        executor.clone(),
    );
    let root = view.root();
    let grid = view.layout(
        &root,
        "f_grid",
        Layout::Grid {
            columns: 3,
            gap: 6.0,
        },
    );
    view.label(&grid, "f_label", Binding::constant("full".into()));
    view.style("f_label", code_style(None, Some(H)));
    view.entry(
        &grid,
        "f_entry",
        model.bind(|s| s.entry_text.clone(), Some(|s, v| s.entry_text = v)),
    );
    view.style("f_entry", code_style(None, Some(H)));
    let go = || {
        model.command(
            |_| true,
            |_, _: String| {},
            Binding::constant("go".to_owned()),
        )
    };
    view.button(&grid, "f_button", Binding::constant("Run".into()), go());
    view.style("f_button", code_style(None, Some(H)));
    view.checkbox(
        &grid,
        "f_check",
        Binding::constant("Check".into()),
        model.bind(|s| s.flag, Some(|s, v| s.flag = v)),
    );
    view.style("f_check", code_style(None, Some(H)));
    view.radio(
        &grid,
        "f_radio",
        Binding::constant("Pick".into()),
        model.bind(|s| s.picked.clone(), Some(|s, v| s.picked = v)),
        "a",
    );
    view.style("f_radio", code_style(None, Some(H)));
    view.slider(
        &grid,
        "f_slider",
        model.bind(|s| s.level, Some(|s, v| s.level = v)),
        SliderRange {
            min: 0.0,
            max: 100.0,
            step: 1.0,
        },
    );
    view.style("f_slider", code_style(None, Some(H)));
    let nest = view.layout(
        &grid,
        "f_nest",
        Layout::Stack {
            horizontal: true,
            gap: 6.0,
        },
    );
    view.style(
        "f_nest",
        Style {
            placement: Placement {
                row: 2,
                column: 0,
                column_span: 2,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    view.label(&nest, "n_label", Binding::constant("n1".into()));
    view.style("n_label", code_style(Some(120.0), Some(H)));
    view.button(&nest, "n_button", Binding::constant("N2".into()), go());
    view.style("n_button", code_style(Some(120.0), Some(H)));
    view.combo(
        &grid,
        "f_combo",
        model.bind(|s| s.combo_items.clone(), None),
        model.bind(|s| s.combo_sel.clone(), Some(|s, v| s.combo_sel = v)),
    );
    view.style(
        "f_combo",
        Style {
            placement: Placement {
                row: 2,
                column: 2,
                height: Some(H),
                ..Default::default()
            },
            text: Some(TextStyle {
                role: TextRole::Code,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let flex = view.layout(&root, "f_flex", Layout::Flex { gap: 6.0 });
    view.label(&flex, "x_label", Binding::constant("fx".into()));
    view.style("x_label", code_style(Some(140.0), Some(H)));
    view.button(&flex, "x_button", Binding::constant("X".into()), go());
    view.style("x_button", code_style(Some(140.0), Some(H)));
    view.entry(
        &flex,
        "x_entry",
        model.bind(|s| s.entry_text.clone(), Some(|s, v| s.entry_text = v)),
    );
    view.style("x_entry", code_style(Some(160.0), Some(H)));
    let abs = view.layout(&root, "f_abs", Layout::Absolute);
    view.label(&abs, "a_label", Binding::constant("abs".into()));
    view.style(
        "a_label",
        Style {
            placement: Placement {
                x: 10.0,
                y: 6.0,
                width: Some(150.0),
                height: Some(H),
                ..Default::default()
            },
            text: Some(TextStyle {
                role: TextRole::Code,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    view.button(&abs, "a_button", Binding::constant("Abs".into()), go());
    view.style(
        "a_button",
        Style {
            placement: Placement {
                x: 180.0,
                y: 6.0,
                width: Some(150.0),
                height: Some(H),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let row = view.layout(
        &root,
        "f_row",
        Layout::Stack {
            horizontal: true,
            gap: 6.0,
        },
    );
    view.meter(&row, "m_meter", model.bind(|s| s.progress, None));
    view.style("m_meter", code_style(Some(170.0), Some(H)));
    view.image(&row, "m_image", model.bind(|s| s.picture.clone(), None));
    view.style("m_image", code_style(Some(H), Some(H)));
    view.separator(&row, "m_sep", true);
    view.style("m_sep", code_style(Some(10.0), Some(H)));
    let tabs = view.tab(&root, "f_tabs", Binding::constant("p0".into()));
    let page0 = view.tab_page(&tabs, "fp0", Binding::constant("P0".into()), "p0");
    view.label(&page0, "t_label", Binding::constant("Tab zero".into()));
    view.style("t_label", code_style(None, Some(H)));
    let page1 = view.tab_page(&tabs, "fp1", Binding::constant("P1".into()), "p1");
    let tgrid = view.layout(
        &page1,
        "t_grid",
        Layout::Grid {
            columns: 2,
            gap: 6.0,
        },
    );
    view.label(&tgrid, "t1_label", Binding::constant("t1".into()));
    view.style("t1_label", code_style(None, Some(H)));
    view.entry(
        &tgrid,
        "t2_entry",
        model.bind(|s| s.entry_text.clone(), Some(|s, v| s.entry_text = v)),
    );
    view.style("t2_entry", code_style(None, Some(H)));
    view.finish_tab(&tabs).unwrap();
    let scr = view.try_scroll(&root, "f_scr", false).unwrap();
    view.label(&scr, "s_label", Binding::constant("scr".into()));
    view.style("s_label", code_style(None, Some(H)));
    view.entry(
        &scr,
        "s_entry",
        model.bind(|s| s.entry_text.clone(), Some(|s, v| s.entry_text = v)),
    );
    view.style("s_entry", code_style(None, Some(H)));
    view.items(
        &root,
        "f_rows",
        model.bind(|s| s.rows.clone(), None),
        Binding::constant(Viewport {
            start: 0,
            count: 3,
            row_height: 44.0,
        }),
        Rc::new(
            |view: &V, parent: &V::Parent, prefix: &str, row: Store<FullRow>| {
                let line = view.layout(
                    parent,
                    &format!("{prefix}/line"),
                    Layout::Stack {
                        horizontal: true,
                        gap: 6.0,
                    },
                );
                view.label(
                    &line,
                    &format!("{prefix}/title"),
                    row.bind(|s| s.title.clone(), None),
                );
                view.style(&format!("{prefix}/title"), code_style(Some(150.0), Some(H)));
                view.entry(
                    &line,
                    &format!("{prefix}/note"),
                    row.bind(|s| s.note.clone(), Some(|s, v| s.note = v)),
                );
                view.style(&format!("{prefix}/note"), code_style(Some(190.0), Some(H)));
                view.checkbox(
                    &line,
                    &format!("{prefix}/flag"),
                    Binding::constant("F".into()),
                    row.bind(|s| s.flag, Some(|s, v| s.flag = v)),
                );
                view.style(&format!("{prefix}/flag"), code_style(Some(100.0), Some(H)));
            },
        ),
    );
    view.content(
        &root,
        "f_detail",
        model.bind(|s| s.detail.clone(), None),
        Rc::new(
            |view: &V, parent: &V::Parent, prefix: &str, item: Store<String>| {
                view.label(
                    parent,
                    &format!("{prefix}/shown"),
                    item.bind(Clone::clone, None),
                );
                view.style(&format!("{prefix}/shown"), code_style(Some(200.0), Some(H)));
            },
        ),
    );
    view.table(&root, "f_tbl", model.bind(|s| s.table.clone(), None));
    let noop = || model.command(|_| true, |_, ()| {}, Binding::constant(()));
    view.dialog_footer(
        &root,
        "f_foot",
        DialogFooter {
            accept: FooterButton {
                text: Binding::constant("Save".into()),
                command: noop(),
            },
            cancel: FooterButton {
                text: Binding::constant("Cancel".into()),
                command: noop(),
            },
            destructive: false,
        },
    );
    FullCorpus { model }
}
