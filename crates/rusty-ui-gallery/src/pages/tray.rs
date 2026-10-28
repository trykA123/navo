use super::{Page, PageInfo, info};

pub(super) const WIDE: PageInfo = info(Page::WorkspaceTray, "Workspace tray", "w5-tray", false);

pub(super) const PHONE: PageInfo = info(
    Page::WorkspaceTrayPhone,
    "Workspace tray, 390",
    "w5-tray-390",
    false,
);
