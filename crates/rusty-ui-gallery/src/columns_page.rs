use rusty_ui::prelude::*;
use rusty_ui::primitives::{ColumnAlign, ColumnEvent, ColumnHead, ColumnSet, ColumnSpec};
use rusty_ui::tokens::{self, Theme};

const ROWS: [[&str; 5]; 10] = [
    [
        "Ifx_Ssw_Tc0_StartupSequence",
        "0x8000_0100",
        "1.2 KB",
        "Func",
        ".text",
    ],
    [
        "Can_17_McmCan_MainFunction_Write",
        "0x8001_4A20",
        "640 B",
        "Func",
        ".text",
    ],
    [
        "EcuM_GlobalConfigRoot",
        "0x8003_0000",
        "96 B",
        "Object",
        ".rodata",
    ],
    ["Os_TaskTable", "0x7000_0400", "2.0 KB", "Object", ".data"],
    [
        "Dem_EventMemoryPrimary",
        "0x7000_1C00",
        "12 KB",
        "Object",
        ".bss",
    ],
    ["main", "0x8000_2000", "312 B", "Func", ".text"],
    [
        "Rte_Write_Engine_Speed",
        "0x8002_0F10",
        "48 B",
        "Func",
        ".text",
    ],
    ["__stack_top", "0x7003_FFF0", "0 B", "NoType", "ABS"],
    [
        "NvM_BlockDescriptorTable",
        "0x8003_1200",
        "4.5 KB",
        "Object",
        ".rodata",
    ],
    [
        "Com_IPdu_TxBuffer",
        "0x7000_8800",
        "768 B",
        "Object",
        ".bss",
    ],
];

fn spec(
    title: &'static str,
    (min_chars, max_chars): (usize, usize),
    align: ColumnAlign,
) -> ColumnSpec {
    ColumnSpec {
        id: title.into(),
        title: title.into(),
        align,
        min_chars,
        max_chars,
        flex: false,
    }
}

fn specs() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec {
            flex: true,
            ..spec("Name", (16, 80), ColumnAlign::Start)
        },
        spec("Address", (8, 24), ColumnAlign::Start),
        spec("Size", (6, 12), ColumnAlign::End),
        spec("Type", (5, 10), ColumnAlign::Start),
        spec("Section", (6, 16), ColumnAlign::Start),
    ]
}

pub struct ColumnsPage {
    set: Entity<ColumnSet>,
    sorted: (usize, bool),
}

impl ColumnsPage {
    pub fn new(window: &Window, cx: &mut Context<Self>) -> Self {
        let set = cx.new(|cx| {
            let mut set = ColumnSet::new(specs(), window, cx);
            set.fit_content(ROWS.iter().map(|row| row.map(str::to_owned).to_vec()));
            set
        });
        cx.subscribe(&set, |page, _, event: &ColumnEvent, cx| {
            if let ColumnEvent::Sort(at) = *event {
                let (sorted, descending) = page.sorted;
                page.sorted = (at, sorted == at && !descending);
            }
            cx.notify();
        })
        .detach();
        Self {
            set,
            sorted: (0, false),
        }
    }
}

fn row(t: &Theme, set: &ColumnSet, (index, cells): (usize, &[&str; 5])) -> Div {
    div()
        .flex()
        .w_full()
        .h(t.spacing.row_height)
        .when(index % 2 == 1, |line| line.bg(t.colors.zebra))
        .children(cells.iter().enumerate().map(|(at, text)| {
            set.cell(at)
                .when(at == 1, |cell| {
                    cell.font_family(t.typography.mono_font.clone())
                })
                .when(at == 2, |cell| {
                    cell.font_family(t.typography.numbers_font.clone())
                })
                .child(div().min_w_0().truncate().child((*text).to_owned()))
        }))
}

impl Render for ColumnsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let (sorted, descending) = self.sorted;
        let mut rows = ROWS;
        rows.sort_by(|a, b| a[sorted].cmp(b[sorted]));
        if descending {
            rows.reverse();
        }
        let set = self.set.read(cx);
        let lines: Vec<Div> = rows
            .iter()
            .enumerate()
            .map(|line| row(&t, set, line))
            .collect();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .child(ColumnHead::new("columns-head", &self.set).sorted(sorted, descending))
            .children(lines)
    }
}
