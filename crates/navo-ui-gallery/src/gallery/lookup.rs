use navo_ui::prelude::*;

use super::Gallery;
use crate::pages::Page;
use crate::viewer_menu_page::ViewerMenuPage;
use crate::window_samples::{job_element, parts_element, preview_element, window_element};

impl Gallery {
    pub(super) fn git_page(&self) -> Option<AnyElement> {
        crate::git_badges_page::element(self.page, &self.git_badges)
            .or_else(|| crate::git_menu_page::element(self.page, &self.git_menu))
    }

    pub(super) fn window_page(&self) -> Option<AnyElement> {
        parts_element(self.page, &self.parts_windows)
            .or_else(|| preview_element(self.page, &self.preview_windows))
            .or_else(|| job_element(self.page, &self.job_windows))
            .or_else(|| window_element(self.page, self.windows()))
    }

    pub(super) fn viewer_menu(&self) -> Option<Entity<ViewerMenuPage>> {
        match self.page {
            Page::ViewerMenuText => Some(self.viewer_menus[0].clone()),
            Page::ViewerMenuTree => Some(self.viewer_menus[1].clone()),
            Page::ViewMenuText => Some(self.viewer_menus[2].clone()),
            Page::ViewerEncoding => Some(self.viewer_menus[3].clone()),
            Page::ViewMenuAutomotive => Some(self.viewer_menus[4].clone()),
            Page::ViewMenuElf => Some(self.viewer_menus[5].clone()),
            Page::ViewMenuTrace => Some(self.viewer_menus[6].clone()),
            Page::ViewMenuMarkdown => Some(self.viewer_menus[7].clone()),
            _ => None,
        }
    }
}
