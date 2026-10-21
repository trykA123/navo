use crate::drag_page::DragPage;
use crate::multi_rename_page::MultiRenamePage;
use crate::multi_rename_preview_page::{PreviewPages, preview_pages};
use crate::multi_rename_tool_page::{ToolPages, tool_pages};
use crate::pages::Page;
use crate::rich_page::{RICH_PAGES, RichPage};
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
        reserved: None,
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

pub type DragTreemapBars = (
    Entity<DragPage>,
    Entity<TreemapPage>,
    Entity<BarsPage>,
    Entity<MultiRenamePage>,
    PreviewPages,
    ToolPages,
    [Entity<RichPage>; 10],
);

pub fn drag_treemap_bars(window: &mut Window, cx: &mut App) -> DragTreemapBars {
    (
        cx.new(DragPage::new),
        cx.new(|_| TreemapPage::new()),
        cx.new(|_| BarsPage),
        cx.new(|cx| MultiRenamePage::new(window, cx)),
        preview_pages(window, cx),
        tool_pages(window, cx),
        RICH_PAGES.map(|(_, sample, off)| cx.new(|cx| RichPage::new(sample, off, window, cx))),
    )
}

fn rich_page(pages: &DragTreemapBars, page: Page) -> Option<&Entity<RichPage>> {
    let at = RICH_PAGES.iter().position(|(shown, ..)| *shown == page)?;
    pages.6.get(at)
}

pub fn drive_of(pages: &DragTreemapBars, page: Page, window: &mut Window, cx: &mut App) -> bool {
    let Some(rich) = rich_page(pages, page) else {
        return false;
    };
    RichPage::drive(rich, window, cx);
    true
}

pub fn page_of(pages: &DragTreemapBars, page: Page) -> Option<AnyElement> {
    if let Some(rich) = rich_page(pages, page) {
        return Some(rich.clone().into_any_element());
    }
    let (drag, treemap, bars, fields, preview, tool, _) = pages;
    Some(match page {
        Page::Drag => drag.clone().into_any_element(),
        Page::Treemap => treemap.clone().into_any_element(),
        Page::UsageBars => bars.clone().into_any_element(),
        Page::MultiRename => fields.clone().into_any_element(),
        Page::MultiRenamePreview => preview[0].clone().into_any_element(),
        Page::MultiRenamePreviewLarge => preview[1].clone().into_any_element(),
        Page::MultiRenameTool => tool[0].clone().into_any_element(),
        Page::MultiRenameToolMenu => tool[1].clone().into_any_element(),
        _ => return None,
    })
}
