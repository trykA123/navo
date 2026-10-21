use rusty_ui::prelude::*;
use rusty_ui::settings::{Change, Field, Value, Values};
use rusty_ui::window_view::ViewerBody;

use crate::automotive_sample::{ELF_NAME, elf};
use crate::map_sample::{MAP_NAME, map};
use crate::pages::Page;
use crate::structured_sample::binary_beside;

const MDF_NAME: &str = "bcm_app.mdf";
const ELF_MDF: &str = "\
# invented memory definition for the gallery
space linear 1 8
chip pflash0 1 8 0x4000 rom
chip dsram0 2 8 0x2000 ram
map 1 0x8000 0x4000 1 0
map 1 0x20000000 0x2000 2 0
";
const MAP_MDF: &str = "\
# invented memory definition for the gallery map
space linear 1 8
chip mpe:pflash0 1 8 0x200000 rom
chip mpe:dflash0 2 8 0x10000 rom
chip mpe:dsram0 3 8 0x20000 ram
chip mpe:psram0 4 8 0x10000 ram
map 1 0x80000000 0x200000 1 0
map 1 0xAF000000 0x10000 2 0
map 1 0x70000000 0x20000 3 0
map 1 0x70100000 0x10000 4 0
";

#[derive(Clone, Copy)]
pub enum Sample {
    Elf,
    Map,
}

const PARTS: [Field; 4] = [
    Field::AutomotiveMemoryStrip,
    Field::AutomotiveUsageBars,
    Field::AutomotiveWhoUses,
    Field::AutomotiveRowBars,
];

pub const RICH_PAGES: [(Page, Sample, Option<Field>); 10] = [
    (Page::ElfRich, Sample::Elf, None),
    (
        Page::ElfNoStrip,
        Sample::Elf,
        Some(Field::AutomotiveMemoryStrip),
    ),
    (
        Page::ElfNoBars,
        Sample::Elf,
        Some(Field::AutomotiveUsageBars),
    ),
    (Page::ElfNoWho, Sample::Elf, Some(Field::AutomotiveWhoUses)),
    (
        Page::ElfNoRowBars,
        Sample::Elf,
        Some(Field::AutomotiveRowBars),
    ),
    (Page::MapRich, Sample::Map, None),
    (
        Page::MapNoStrip,
        Sample::Map,
        Some(Field::AutomotiveMemoryStrip),
    ),
    (
        Page::MapNoBars,
        Sample::Map,
        Some(Field::AutomotiveUsageBars),
    ),
    (Page::MapNoWho, Sample::Map, Some(Field::AutomotiveWhoUses)),
    (
        Page::MapNoRowBars,
        Sample::Map,
        Some(Field::AutomotiveRowBars),
    ),
];

fn values(off: Option<Field>) -> Values {
    let pick = |at| Change {
        field: Field::AutomotivePreset,
        value: Value::Pick(at),
    };
    let Some(off) = off else {
        return Values::default().with(&pick(1));
    };
    PARTS.into_iter().fold(
        Values::default().with(&pick(2)).with(&Change {
            field: Field::AutomotiveInspector,
            value: Value::Flag(true),
        }),
        |values, field| {
            values.with(&Change {
                field,
                value: Value::Flag(field != off),
            })
        },
    )
}

pub struct RichPage {
    body: Option<Box<dyn ViewerBody>>,
}

impl RichPage {
    pub fn new(sample: Sample, off: Option<Field>, window: &mut Window, cx: &mut App) -> Self {
        let body = match sample {
            Sample::Elf => binary_beside((ELF_NAME, elf()), (MDF_NAME, ELF_MDF), window, cx),
            Sample::Map => binary_beside(
                (MAP_NAME, map().as_bytes()),
                (MDF_NAME, MAP_MDF),
                window,
                cx,
            ),
        };
        if let Some(body) = &body {
            body.show_parts(&values(off), cx);
        }
        Self { body }
    }

    pub fn drive(page: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let Some(focus) = page
            .read(cx)
            .body
            .as_ref()
            .map(|body| body.focus_handle(cx))
        else {
            return;
        };
        window.focus(&focus, cx);
        if let Ok(action) = cx.build_action("viewer::RightDrawer", None) {
            window.dispatch_action(action, cx);
        }
    }
}

impl Render for RichPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match &self.body {
            Some(body) => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(body.view()),
            None => div().child("This sample did not load."),
        }
    }
}
