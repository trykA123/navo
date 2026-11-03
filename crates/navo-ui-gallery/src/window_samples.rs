use navo_ui::file_list::{SortBy, SortKey};
use navo_ui::fkey_bar::KeyLayer;
use navo_ui::modifier_hints::Combo;
use navo_ui::prelude::*;
use navo_ui::primitives::text_field_state;
use navo_ui::settings::{Field, WindowParts};
use navo_ui::side_sheet::Side;
use navo_ui::window_view::{UpdateFailure, UpdateView};

use crate::gallery::Page;
use crate::settings_sample::Extras;
use crate::shortcuts_sample::{self, Shows as ShortcutsShows};
use crate::window_page::{HintsShown, JobsLook, JobsShown, SheetShows, WindowPage};

pub fn job_windows(cx: &mut App) -> [Entity<WindowPage>; 7] {
    [
        (1, JobsLook::Card),
        (1, JobsLook::Strip),
        (2, JobsLook::Card),
        (2, JobsLook::Strip),
        (4, JobsLook::Drawer),
        (6, JobsLook::Strip),
        (6, JobsLook::Drawer),
    ]
    .map(|(count, look)| cx.new(|cx| WindowPage::with_jobs(JobsShown { count, look }, cx)))
}

pub fn job_element(page: Page, windows: &[Entity<WindowPage>; 7]) -> Option<AnyElement> {
    let [job, job_strip, two, two_strip, drawer, six, six_drawer] = windows;
    let shown = match page {
        Page::WindowJob => job,
        Page::WindowJobStrip => job_strip,
        Page::WindowJobs => two,
        Page::WindowJobsStrip => two_strip,
        Page::WindowActivityDrawer => drawer,
        Page::SixStrip => return Some(fixed(SIX_WIDE, six)),
        Page::Six390 => return Some(fixed(PHONE, six)),
        Page::SixDrawer => return Some(fixed(PHONE, six_drawer)),
        _ => return None,
    };
    Some(shown.clone().into_any_element())
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
            extras: Extras::default(),
            shortcuts,
        };
        cx.new(|cx| WindowPage::with_sheet(shows, cx))
    })
}

pub fn settings_windows(window: &mut Window, cx: &mut App) -> [Entity<WindowPage>; 7] {
    let input = text_field_state("Find a setting by name, key or help", window, cx);
    input.update(cx, |state, cx| state.set_value("sort", window, cx));
    let progress = text_field_state("Find a setting by name, key or help", window, cx);
    progress.update(cx, |state, cx| state.set_value("progress", window, cx));
    [
        (Field::AutomotiveModules, false, None, true, None),
        (Field::Theme, true, None, false, None),
        (Field::Sort, false, Some((input, "sort")), false, None),
        (Field::SearchIndex, false, None, true, None),
        (
            Field::SearchIndex,
            false,
            Some((progress, "progress")),
            false,
            None,
        ),
        (Field::GitTool, false, None, false, None),
        (Field::GitToolCommand, false, None, false, Some(MINIMUM)),
    ]
    .map(
        |(field, has_notice, find, is_scrolled_to_end, window_width)| {
            let shows = SheetShows {
                active: Side::Left,
                window_width,
                is_ringed: false,
                settings: Some(field),
                extras: Extras {
                    has_notice,
                    find,
                    is_scrolled_to_end,
                    update: None,
                },
                shortcuts: None,
            };
            cx.new(|cx| WindowPage::with_sheet(shows, cx))
        },
    )
}

pub fn settings_element(page: Page, windows: &[Entity<WindowPage>; 7]) -> Option<AnyElement> {
    let [display, upgrade, find, search, index, git, git_minimum] = windows;
    let shown = match page {
        Page::SettingsDisplay => display,
        Page::SettingsUpgrade => upgrade,
        Page::SettingsFind => find,
        Page::SearchSettings => search,
        Page::IndexProgress => index,
        Page::SettingsGit => git,
        Page::SettingsGitMinimum => return Some(fixed(MINIMUM, git_minimum)),
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}

pub fn help_windows(cx: &mut App) -> [Entity<WindowPage>; 3] {
    let downloading = UpdateView::Downloading {
        version: "0.4.0".into(),
        percent: 40,
    };
    let limited = UpdateView::Failed(UpdateFailure::RateLimited {
        next_try: "14:30".into(),
    });
    [
        (None, downloading),
        (Some(PHONE), limited),
        (None, UpdateView::Hidden),
    ]
    .map(|(window_width, update)| {
        let shows = SheetShows {
            active: Side::Left,
            window_width,
            is_ringed: false,
            settings: Some(Field::CheckUpdates),
            extras: Extras {
                update: Some(update),
                ..Extras::default()
            },
            shortcuts: None,
        };
        cx.new(|cx| WindowPage::with_sheet(shows, cx))
    })
}

pub fn help_element(page: Page, windows: &[Entity<WindowPage>; 3]) -> Option<AnyElement> {
    let [desktop, phone, unpackaged] = windows;
    match page {
        Page::SettingsHelpUpdates => Some(desktop.clone().into_any_element()),
        Page::SettingsHelpUpdatesPhone => Some(fixed(PHONE, phone)),
        Page::SettingsHelpUnpackaged => Some(unpackaged.clone().into_any_element()),
        _ => None,
    }
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
const SIX_WIDE: f32 = 1280.;
const PHONE: f32 = 390.;
const MINIMUM: f32 = 480.;

fn fixed(width: f32, window: &Entity<WindowPage>) -> AnyElement {
    div()
        .w(px(width))
        .h_full()
        .flex_none()
        .child(window.clone())
        .into_any_element()
}

pub fn window_element(page: Page, windows: [&Entity<WindowPage>; 20]) -> Option<AnyElement> {
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
        hints @ ..,
    ] = windows;
    let [hints_ctrl, hints_shift, hints_alt, hints_low, notices @ ..] = hints;
    let shown = match page {
        Page::Window => main,
        Page::WindowShift => shift,
        Page::WindowCtrl => ctrl,
        Page::WindowAlt => alt,
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
        Page::CrashNotice => notices[0],
        Page::RecoveryNotice => notices[1],
        Page::RecoveryNoticeNarrow => return Some(fixed(NARROW, notices[1])),
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}

pub fn notice_windows(cx: &mut App) -> [Entity<WindowPage>; 2] {
    [
        cx.new(WindowPage::with_crash),
        cx.new(WindowPage::with_recovery),
    ]
}

pub fn parts_windows(cx: &mut App) -> [Entity<WindowPage>; 3] {
    let by_size = SortKey {
        by: SortBy::Size,
        descending: true,
    };
    [
        (WindowParts::all(true), false, SortKey::default()),
        (WindowParts::all(false), false, by_size),
        (WindowParts::all(true), true, SortKey::default()),
    ]
    .map(|(parts, shows_menu, sort)| {
        cx.new(|cx| WindowPage::with_parts(parts, shows_menu, cx).sorted_by(sort))
    })
}

pub fn preview_windows(cx: &mut App) -> [Entity<WindowPage>; 4] {
    [(true, true), (false, true), (true, false), (false, false)].map(|parts| {
        let shows = SheetShows {
            active: Side::Left,
            window_width: None,
            is_ringed: false,
            settings: None,
            shortcuts: None,
            extras: Extras::default(),
        };
        cx.new(|cx| WindowPage::with_sheet(shows, cx).with_preview_parts(parts))
    })
}

pub fn preview_element(page: Page, windows: &[Entity<WindowPage>; 4]) -> Option<AnyElement> {
    let [on, no_bars, no_largest, off] = windows;
    let shown = match page {
        Page::PreviewPartsOn => on,
        Page::PreviewNoBars => no_bars,
        Page::PreviewNoLargest => no_largest,
        Page::PreviewPartsOff => off,
        _ => return None,
    };
    Some(shown.clone().into_any_element())
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
