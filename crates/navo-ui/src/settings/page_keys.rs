//! Ctrl+1 to Ctrl+9 and Ctrl+0 in Settings: each jumps straight to the page with that
//! digit in the nav.

use gpui_kit::Entity;

use super::fields::{Field, Section};
use super::{SettingsBody, SheetEvent, on_event_dismissing};
use crate::prelude::*;
use crate::primitives::popup_menu::PopupMenu;

gpui_kit::actions!(
    settings_sheet,
    [
        PageAppearance,
        PageWindow,
        PageFiles,
        PageCopyDelete,
        PageSearch,
        PageViewers,
        PageTools,
        PageTrayClipboard,
        PageKeyboard,
        PageAbout
    ]
);

/// Handler that shows `page` at its first shown row; it does nothing while a find hides
/// the whole page.
fn go_to(
    body: &SettingsBody,
    page: Section,
    menus: &[(Field, Entity<PopupMenu>)],
) -> impl Fn(&mut Window, &mut App) + use<> {
    on_event_dismissing(
        body,
        body.filter.first_in(page).map(SheetEvent::Select),
        menus,
    )
}

/// Adds the ten page keys to the sheet's key root.
pub(super) fn register(
    root: Div,
    body: &SettingsBody,
    menus: &[(Field, Entity<PopupMenu>)],
) -> Div {
    let [
        appearance,
        window,
        files,
        copy,
        search,
        viewers,
        tools,
        tray,
        keyboard,
        about,
    ] = Section::ALL.map(|page| go_to(body, page, menus));
    root.on_action(move |_: &PageAppearance, w, cx| appearance(w, cx))
        .on_action(move |_: &PageWindow, w, cx| window(w, cx))
        .on_action(move |_: &PageFiles, w, cx| files(w, cx))
        .on_action(move |_: &PageCopyDelete, w, cx| copy(w, cx))
        .on_action(move |_: &PageSearch, w, cx| search(w, cx))
        .on_action(move |_: &PageViewers, w, cx| viewers(w, cx))
        .on_action(move |_: &PageTools, w, cx| tools(w, cx))
        .on_action(move |_: &PageTrayClipboard, w, cx| tray(w, cx))
        .on_action(move |_: &PageKeyboard, w, cx| keyboard(w, cx))
        .on_action(move |_: &PageAbout, w, cx| about(w, cx))
}
