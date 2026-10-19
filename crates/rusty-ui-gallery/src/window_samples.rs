use rusty_ui::fkey_bar::KeyLayer;
use rusty_ui::modifier_hints::Combo;
use rusty_ui::prelude::*;
use rusty_ui::settings::Field;
use rusty_ui::side_sheet::Side;

use crate::shortcuts_sample::{self, Shows as ShortcutsShows};
use crate::window_page::{HintsShown, JobsShown, NARROW, SheetShows, WIDE, WindowPage};

pub fn job_windows(cx: &mut App) -> [Entity<WindowPage>; 4] {
    [(1, false), (1, true), (2, false), (2, true)].map(|(count, is_pill)| {
        cx.new(|cx| WindowPage::with_jobs(JobsShown { count, is_pill }, cx))
    })
}

pub fn sheet_windows(window: &mut Window, cx: &mut App) -> [Entity<WindowPage>; 9] {
    let shortcuts = [
        ShortcutsShows::Normal,
        ShortcutsShows::Recording,
        ShortcutsShows::Conflict,
        ShortcutsShows::Invalid,
    ]
    .map(|shows| Some(shortcuts_sample::page(shows, window, cx)));
    let [normal, recording, conflict, invalid] = shortcuts;
    let shows = [
        (Side::Left, Some(WIDE), false, None, None),
        (Side::Right, Some(WIDE), true, None, None),
        (Side::Left, Some(NARROW), false, None, None),
        (Side::Left, None, false, Some(Field::Density), None),
        (Side::Left, None, false, Some(Field::HiddenFiles), None),
        (Side::Left, None, false, None, normal),
        (Side::Left, None, false, None, recording),
        (Side::Left, None, false, None, conflict),
        (Side::Left, None, false, None, invalid),
    ];
    shows.map(|(active, window_width, is_ringed, settings, shortcuts)| {
        let shows = SheetShows {
            active,
            window_width,
            is_ringed,
            settings,
            shortcuts,
        };
        cx.new(|cx| WindowPage::with_sheet(shows, cx))
    })
}

pub fn hint_windows(cx: &mut App) -> [Entity<WindowPage>; 4] {
    [
        (Combo::CTRL, KeyLayer::Ctrl, 2),
        (Combo::SHIFT, KeyLayer::Shift, 2),
        (Combo::ALT, KeyLayer::Alt, 2),
        (Combo::CTRL, KeyLayer::Ctrl, 12),
    ]
    .map(|(combo, held, cursor)| {
        cx.new(|cx| WindowPage::with_hints(HintsShown { combo, cursor }, held, cx))
    })
}
