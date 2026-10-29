use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::keys::{Input, OPEN_ID, QUIT_ID};

const APP_ICON: u16 = 1;

pub(super) fn create() -> Option<TrayIcon> {
    let menu = Menu::new();
    let open = MenuItem::with_id(OPEN_ID, rusty_l10n::tr("cmd-files-open"), true, None);
    let quit = MenuItem::with_id(QUIT_ID, rusty_l10n::tr("cmd-app-quit"), true, None);
    let built = menu
        .append_items(&[&open, &quit])
        .map_err(|error| error.to_string())
        .and_then(|()| Icon::from_resource(APP_ICON, None).map_err(|error| error.to_string()))
        .and_then(|icon| {
            TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip(rusty_core::brand::NAME)
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .map_err(|error| error.to_string())
        });
    built
        .inspect_err(|error| tracing::warn!(%error, "the tray icon could not be created"))
        .ok()
}

pub(super) fn drain(mut each: impl FnMut(Input<'_>)) {
    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            each(Input::LeftClick);
        }
    }
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        each(Input::Menu(event.id.as_ref()));
    }
}
