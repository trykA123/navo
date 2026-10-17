use std::sync::Arc;

use rusty_ui::commands::{Keymap, set_active};
use rusty_ui::prelude::*;
use rusty_ui::settings::{self, SettingsBody, Values};
use rusty_ui::shortcuts::{Recorded, ShortcutsPage};
use rusty_ui::side_sheet::{SheetFrame, SheetState, SideSheet};

const USER: &str = "[[bind]]\ncommand = \"app_menu.focus_left\"\nkeys = [\"h\"]\n";
const PROBLEM: &str = "keymap.toml, line 7: unknown command `files.teleport`";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shows {
    Normal,
    Recording,
    Conflict,
    Invalid,
}

pub fn use_user_keymap(cx: &mut App) {
    if let Ok(overlaid) = Keymap::builtin().overlay(USER) {
        set_active(cx, overlaid.keymap);
    }
}

pub fn page(shows: Shows, window: &mut Window, cx: &mut App) -> Entity<ShortcutsPage> {
    let save = Arc::new(|_: &[_]| Err("The gallery does not save.".to_owned()));
    cx.new(|cx| {
        let mut page = ShortcutsPage::new(save, window, cx);
        match shows {
            Shows::Normal => page.select("app_menu.focus_left", cx),
            Shows::Recording => {
                page.select("files.sort_by_size", cx);
                page.start_recording(cx);
            }
            Shows::Conflict => {
                page.select("files.sort_by_size", cx);
                page.start_recording(cx);
                page.take(Recorded::Key("ctrl-f5".to_owned()), cx);
            }
            Shows::Invalid => page.set_problem(Some(PROBLEM.into()), cx),
        }
        page
    })
}

pub fn sheet(
    state: &Entity<SheetState>,
    frame: SheetFrame,
    page: &Entity<ShortcutsPage>,
) -> SideSheet {
    let body =
        SettingsBody::new(Values::default(), 0).shortcuts(Some(page.clone().into_any_element()));
    settings::sheet(state, frame, body).close("escape", |_, _| {})
}
