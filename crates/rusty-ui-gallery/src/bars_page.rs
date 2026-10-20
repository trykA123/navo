use crate::drag_page::DragPage;
use crate::treemap_page::TreemapPage;
use rusty_automotive::Section;
use rusty_automotive::space::{Kind, Region, RegionSource};
use rusty_ui::prelude::*;
use rusty_ui::tokens;
use rusty_ui::window_view::{FileBytesBar, Legend, RegionHead, UsageBar, UsageLine};

const MIB: u64 = 1 << 20;
const FLASH: u64 = 0x8000_0000;

fn region(name: &str, total: Option<u64>, by_kind: [u64; Kind::COUNT]) -> Region {
    Region {
        name: name.into(),
        start: Some(FLASH),
        start_estimated: false,
        total,
        chip: None,
        used: by_kind[..Kind::Fill.index()].iter().sum(),
        by_kind,
        blocks: Vec::new(),
        source: if total.is_some() {
            RegionSource::Mdf
        } else {
            RegionSource::Clusters
        },
    }
}

fn regions() -> [(Region, Option<u64>); 3] {
    [
        (
            region(
                "pflash0",
                Some(4 * MIB),
                [1_900_000, 620_000, 0, 0, 60_000, 90_000],
            ),
            Some(64 * 1024),
        ),
        (
            region("dsram0", Some(MIB), [0, 0, 180_000, 310_000, 0, 0]),
            None,
        ),
        (region("dflash", None, [0, 96_000, 0, 0, 0, 0]), None),
    ]
}

fn section(name: &str, kind: u32, flags: u64, size: u64) -> Section {
    Section {
        name: name.into(),
        kind,
        flags,
        address: 0,
        size,
        offset: 0,
    }
}

fn sections() -> Vec<Section> {
    vec![
        section(".text", 1, 6, 1_900_000),
        section(".rodata", 1, 2, 620_000),
        section(".data", 1, 3, 180_000),
        section(".debug_info", 1, 0, 1_400_000),
        section(".debug_line", 1, 0, 260_000),
        section(".note.build", 7, 0, 4_000),
        section(".symtab", 2, 0, 210_000),
        section(".strtab", 3, 0, 380_000),
        section(".comment", 1, 0, 2_000),
    ]
}

pub struct BarsPage;

impl Render for BarsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let rows = regions()
            .into_iter()
            .enumerate()
            .map(|(index, (region, reserved))| {
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(t.spacing.xl)
                    .child(
                        div()
                            .w(px(220.))
                            .flex_none()
                            .child(RegionHead::new(&region)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(t.spacing.xs)
                            .flex_1()
                            .min_w(px(260.))
                            .child(UsageBar::new(("usage-bar", index), &region))
                            .child(UsageLine::new(&region, reserved)),
                    )
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .gap(t.spacing.xl)
            .p(t.spacing.xl)
            .bg(t.colors.surface)
            .child(Legend::new().with_fill(true))
            .children(rows)
            .child(FileBytesBar::new(&sections()))
    }
}

pub type DragTreemapBars = (Entity<DragPage>, Entity<TreemapPage>, Entity<BarsPage>);

pub fn drag_treemap_bars(cx: &mut App) -> DragTreemapBars {
    (
        cx.new(DragPage::new),
        cx.new(|_| TreemapPage::new()),
        cx.new(|_| BarsPage),
    )
}
