use std::rc::Rc;
use std::sync::Arc;

use rusty_automotive::space::{
    Block, Kind, NameRef, Owner, OwnerRef, Owners, Region, RegionSource, ScaleMode,
};
use rusty_ui::prelude::*;
use rusty_ui::tokens;
use rusty_ui::window_view::{MemoryStrip, StripData, StripItem, StripLabel};

const FLASH: u64 = 0x8000_0000;
const KINDS: [Kind; 4] = [Kind::Code, Kind::Rodata, Kind::Data, Kind::Bss];

fn block(start: u64, size: u64, kind: Kind, at: usize) -> Block {
    Block {
        start,
        size,
        kind,
        owner: OwnerRef::ElfSection(at),
    }
}

fn region(name: &str, total: u64, blocks: Vec<Block>) -> Region {
    let mut by_kind = [0; Kind::COUNT];
    for block in &blocks {
        by_kind[block.kind.index()] += block.size;
    }
    Region {
        name: name.into(),
        start: blocks.first().map(|block| block.start),
        start_estimated: false,
        total: Some(total),
        chip: None,
        reserved: None,
        used: blocks.iter().map(|block| block.size).sum(),
        by_kind,
        blocks,
        source: RegionSource::Mdf,
    }
}

fn split_flash() -> Region {
    let sizes = [0x6_0000, 0x1_8000, 0x800, 0x2_4000, 0x400, 0x1_0000, 0xC000];
    let mut at = FLASH;
    let mut blocks = Vec::new();
    for (index, size) in sizes.into_iter().enumerate() {
        blocks.push(block(at, size, KINDS[index % 2], index));
        at += size + if index == 3 { 0x2_0000 } else { 0x100 };
    }
    region("pflash0", 0x40_0000, blocks)
}

fn merged_flash() -> (Region, Owners) {
    let text = block(FLASH, 0x20_0000, Kind::Code, 0);
    let rodata = block(FLASH + 0x20_0000, 0x8_0000, Kind::Rodata, 1);
    let mut owners = Owners::default();
    let mut at = FLASH;
    for index in 0..48u32 {
        let size = 0x4000 + u64::from(index % 7) * 0x1800;
        owners.items.push(Owner {
            start: at,
            size,
            name: NameRef::Symbol(index),
            kind: Kind::Code,
            section: 0,
            object: Some(index % 5),
            source_file: None,
            estimated: false,
            image: false,
        });
        at += size;
    }
    (region("pflash1", 0x30_0000, vec![text, rodata]), owners)
}

fn tiny_sections() -> Region {
    let mut blocks = Vec::new();
    let mut at = 0x7000_0000;
    for index in 0..3_000 {
        let size = 16 + (index as u64 * 37) % 240;
        blocks.push(block(at, size, KINDS[(index / 300) % 4], index));
        at += size;
    }
    region("dspr0", 0x10_0000, blocks)
}

fn data(region: Region, owners: Option<Owners>) -> StripData {
    let object = |index| ["can.o", "os.o", "dem.o", "nvm.o", "rte.o"][index % 5];
    StripData {
        region: Arc::new(region),
        owners: owners.map(Arc::new),
        describe: Rc::new(move |item| match item {
            StripItem::Block(at) => StripLabel {
                name: format!(".text.Module_{at}").into(),
                object: None,
            },
            StripItem::Owner(at) => StripLabel {
                name: format!("Can_MainFunction_{at}").into(),
                object: Some(object(at).into()),
            },
        }),
    }
}

pub struct StripPage {
    strips: Vec<(&'static str, Entity<MemoryStrip>)>,
}

impl StripPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let strip = |cx: &mut Context<Self>, data: StripData, height: f32, mode: ScaleMode| {
            cx.new(|cx| {
                let mut strip = MemoryStrip::new(data, px(height), cx);
                strip.set_mode(mode, cx);
                strip
            })
        };
        let compressed = strip(cx, data(split_flash(), None), 40., ScaleMode::Compressed);
        compressed.update(cx, |strip, cx| strip.set_caret(Some(3), cx));
        let true_scale = strip(cx, data(split_flash(), None), 28., ScaleMode::True);
        let (merged, owners) = merged_flash();
        let zoomed = strip(cx, data(merged, Some(owners)), 40., ScaleMode::Compressed);
        zoomed.update(cx, |strip, cx| {
            strip.set_caret(Some(0), cx);
            strip.zoom_in(cx);
        });
        let tiny = strip(cx, data(tiny_sections(), None), 28., ScaleMode::Compressed);
        tiny.update(cx, |strip, cx| {
            let lit: Vec<bool> = (0..3_000).map(|at| (600..1_200).contains(&at)).collect();
            strip.set_lit(Some(Rc::from(lit)), cx);
            strip.set_caret(Some(900), cx);
        });
        Self {
            strips: vec![
                ("Compressed gaps, block 3 selected", compressed),
                ("True scale", true_scale),
                ("Zoomed into .text, inner symbols", zoomed),
                ("3,000 small sections merged, one group lit", tiny),
            ],
        }
    }
}

impl Render for StripPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(t.spacing.xxl)
            .p(t.spacing.xxl)
            .bg(t.colors.surface)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .children(self.strips.iter().map(|(title, strip)| {
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.xs)
                    .child(*title)
                    .child(strip.clone())
            }))
    }
}
