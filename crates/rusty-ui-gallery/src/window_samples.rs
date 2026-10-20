use rusty_ui::file_list::{SortBy, SortKey};
use rusty_ui::fkey_bar::KeyLayer;
use rusty_ui::modifier_hints::Combo;
use rusty_ui::prelude::*;
use rusty_ui::settings::{Field, WindowParts};
use rusty_ui::side_sheet::Side;

use crate::gallery::Page;
use crate::shortcuts_sample::{self, Shows as ShortcutsShows};
use crate::window_page::{HintsShown, JobsShown, SheetShows, WindowPage};

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

pub fn display_window(cx: &mut App) -> Entity<WindowPage> {
    let shows = SheetShows {
        active: Side::Left,
        window_width: None,
        is_ringed: false,
        settings: Some(Field::TextLineNumbers),
        shortcuts: None,
    };
    cx.new(|cx| WindowPage::with_sheet(shows, cx))
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

pub const NARROW: f32 = 1100.;
pub const WIDE: f32 = 1440.;

fn fixed(width: f32, window: &Entity<WindowPage>) -> AnyElement {
    div()
        .w(px(width))
        .h_full()
        .flex_none()
        .child(window.clone())
        .into_any_element()
}

pub fn window_element(page: Page, windows: [&Entity<WindowPage>; 23]) -> Option<AnyElement> {
    let [
        main,
        shift,
        ctrl,
        alt,
        menu,
        sheet,
        sheet_left,
        sheet_narrow,
        settings,
        settings_files,
        shortcuts,
        recording,
        conflict,
        invalid,
        job,
        job_pill,
        two,
        two_pill,
        hints @ ..,
    ] = windows;
    let [hints_ctrl, hints_shift, hints_alt, hints_low, crash] = hints;
    let shown = match page {
        Page::Window => main,
        Page::WindowShift => shift,
        Page::WindowCtrl => ctrl,
        Page::WindowAlt => alt,
        Page::WindowJob => job,
        Page::WindowJobPill => job_pill,
        Page::WindowJobs => two,
        Page::WindowJobsPill => two_pill,
        Page::AppMenu => menu,
        Page::WindowNarrow => return Some(fixed(NARROW, main)),
        Page::Sheet => return Some(fixed(WIDE, sheet)),
        Page::SheetLeft => return Some(fixed(WIDE, sheet_left)),
        Page::SheetNarrow => return Some(fixed(NARROW, sheet_narrow)),
        Page::Settings => settings,
        Page::SettingsFiles => settings_files,
        Page::Shortcuts => shortcuts,
        Page::ShortcutsRecording => recording,
        Page::ShortcutsConflict => conflict,
        Page::ShortcutsInvalid => invalid,
        Page::HintsCtrl => hints_ctrl,
        Page::HintsShift => hints_shift,
        Page::HintsAlt => hints_alt,
        Page::HintsLow => hints_low,
        Page::CrashNotice => crash,
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}

pub fn parts_windows(cx: &mut App) -> [Entity<WindowPage>; 3] {
    let by_size = SortKey {
        by: SortBy::Size,
        descending: true,
    };
    [
        (WindowParts::default(), false, SortKey::default()),
        (WindowParts::all(false), false, by_size),
        (WindowParts::default(), true, SortKey::default()),
    ]
    .map(|(parts, shows_menu, sort)| {
        cx.new(|cx| WindowPage::with_parts(parts, shows_menu, cx).sorted_by(sort))
    })
}

pub fn parts_element(page: Page, windows: &[Entity<WindowPage>; 3]) -> Option<AnyElement> {
    let [on, off, menu] = windows;
    let shown = match page {
        Page::WindowPartsOn => on,
        Page::WindowPartsOff => off,
        Page::WindowViewMenu => menu,
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}
