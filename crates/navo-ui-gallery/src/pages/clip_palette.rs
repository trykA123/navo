use super::{Page, PageInfo, info};

pub(super) const WIDE: PageInfo = info(
    Page::ClipPalette,
    "Clipboard palette",
    "clip-palette",
    false,
);

pub(super) const PHONE: PageInfo = info(
    Page::ClipPalette390,
    "Clipboard palette, 390",
    "clip-palette-390",
    false,
);

pub(super) const MENU: PageInfo = info(
    Page::ClipPaletteMenu,
    "Clipboard palette, actions menu",
    "clip-palette-menu",
    false,
);

pub(super) const MENU_PHONE: PageInfo = info(
    Page::ClipPaletteMenu390,
    "Clipboard palette, actions menu, 390",
    "clip-palette-menu-390",
    false,
);

pub(super) const SECRET: PageInfo = info(
    Page::ClipPaletteSecret,
    "Clipboard palette, secret hidden",
    "clip-palette-secret",
    false,
);

pub(super) const SECRET_PHONE: PageInfo = info(
    Page::ClipPaletteSecret390,
    "Clipboard palette, secret hidden, 390",
    "clip-palette-secret-390",
    false,
);

pub(super) const SHOWN: PageInfo = info(
    Page::ClipPaletteShown,
    "Clipboard palette, secret shown",
    "clip-palette-shown",
    false,
);

pub(super) const SHOWN_PHONE: PageInfo = info(
    Page::ClipPaletteShown390,
    "Clipboard palette, secret shown, 390",
    "clip-palette-shown-390",
    false,
);

pub(super) const COUNTDOWN: PageInfo = info(
    Page::ClipPaletteCountdown,
    "Clipboard palette, secret copied",
    "clip-palette-countdown",
    false,
);

pub(super) const COUNTDOWN_PHONE: PageInfo = info(
    Page::ClipPaletteCountdown390,
    "Clipboard palette, secret copied, 390",
    "clip-palette-countdown-390",
    false,
);

pub(super) const SECRET_MENU: PageInfo = info(
    Page::ClipPaletteSecretMenu,
    "Clipboard palette, secret actions menu",
    "clip-palette-secret-menu",
    false,
);

pub(super) const SECRET_MENU_PHONE: PageInfo = info(
    Page::ClipPaletteSecretMenu390,
    "Clipboard palette, secret actions menu, 390",
    "clip-palette-secret-menu-390",
    false,
);

pub(super) const POP_UP: PageInfo = info(
    Page::ClipPopUp,
    "Clipboard pop-up window",
    "clip-popup",
    false,
);

pub(super) const POP_UP_PHONE: PageInfo = info(
    Page::ClipPopUp390,
    "Clipboard pop-up window, 390",
    "clip-popup-390",
    false,
);

pub(super) const POP_UP_MENU: PageInfo = info(
    Page::ClipPopUpMenu,
    "Clipboard pop-up window, actions menu",
    "clip-popup-menu",
    false,
);

pub(super) const MANAGE: PageInfo =
    info(Page::ClipManage, "Clipboard library", "clip-manage", false);

pub(super) const MANAGE_PHONE: PageInfo = info(
    Page::ClipManage390,
    "Clipboard library, 390",
    "clip-manage-390",
    false,
);

pub(super) const ALL: [PageInfo; 17] = [
    WIDE,
    PHONE,
    MENU,
    MENU_PHONE,
    SECRET,
    SECRET_PHONE,
    SHOWN,
    SHOWN_PHONE,
    COUNTDOWN,
    COUNTDOWN_PHONE,
    SECRET_MENU,
    SECRET_MENU_PHONE,
    POP_UP,
    POP_UP_PHONE,
    POP_UP_MENU,
    MANAGE,
    MANAGE_PHONE,
];
