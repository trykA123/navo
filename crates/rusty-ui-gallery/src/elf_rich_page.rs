use rusty_ui::prelude::*;
use rusty_ui::settings::{Change, Field, Value, Values};
use rusty_ui::window_view::ViewerBody;

use crate::automotive_sample::{ELF_NAME, elf};
use crate::pages::Page;
use crate::structured_sample::binary_beside;

const MDF_NAME: &str = "bcm_app.mdf";
const MDF: &str = "\
# invented memory definition for the gallery
space linear 1 8
chip pflash0 1 8 0x4000 rom
chip dsram0 2 8 0x2000 ram
map 1 0x8000 0x4000 1 0
map 1 0x20000000 0x2000 2 0
";

const PARTS: [Field; 4] = [
    Field::AutomotiveMemoryStrip,
    Field::AutomotiveUsageBars,
    Field::AutomotiveWhoUses,
    Field::AutomotiveRowBars,
];

pub const ELF_PAGES: [(Page, Option<Field>); 5] = [
    (Page::ElfRich, None),
    (Page::ElfNoStrip, Some(Field::AutomotiveMemoryStrip)),
    (Page::ElfNoBars, Some(Field::AutomotiveUsageBars)),
    (Page::ElfNoWho, Some(Field::AutomotiveWhoUses)),
    (Page::ElfNoRowBars, Some(Field::AutomotiveRowBars)),
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

pub struct ElfRichPage {
    body: Option<Box<dyn ViewerBody>>,
}

impl ElfRichPage {
    pub fn new(off: Option<Field>, window: &mut Window, cx: &mut App) -> Self {
        let body = binary_beside((ELF_NAME, elf()), (MDF_NAME, MDF), window, cx);
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

impl Render for ElfRichPage {
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
