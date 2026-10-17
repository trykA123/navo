use gpui_kit::FocusHandle;
use rusty_core::Command;

use super::folder::FolderPreview;
use super::spot::{Fold, Spot};
use super::{Preview, Shown};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const BROWSE: &str = "PreviewBrowse";

gpui_kit::actions!(
    preview,
    [
        BrowseUp,
        BrowseDown,
        BrowseFold,
        BrowseUnfold,
        BrowseOpen,
        BrowseView,
        LeaveBrowse,
        LeaveAndSwitch,
        TogglePreview,
        Recount
    ]
);

pub(in crate::window_view) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "preview");
}

pub(in crate::window_view) struct Pick {
    pub spot: Spot,
    pub clicks: usize,
}

impl Preview {
    pub(in crate::window_view) fn is_browsing(&self, window: &Window, cx: &App) -> bool {
        self.is_open && (self.browse.is_focused(window) || self.browse.contains_focused(window, cx))
    }

    pub(super) fn browse_target(&self, cx: &App) -> FocusHandle {
        match &self.shown {
            Some(Shown::File(file)) => file.body.as_ref().map(|body| body.focus_handle(cx)),
            _ => None,
        }
        .unwrap_or_else(|| self.browse.clone())
    }
}

impl FerriteWindow {
    pub(in crate::window_view) fn browse_preview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.preview.is_open {
            self.open_preview(window, cx);
        }
        self.enter_browse(window, cx);
    }

    fn enter_browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(folder) = self.preview.folder_mut() {
            folder.want_spot();
        }
        let target = self.preview.browse_target(cx);
        window.focus(&target, cx);
        cx.notify();
    }

    pub(super) fn pick_spot(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let Some(folder) = self.preview.folder_mut() else {
            return;
        };
        folder.spot = Some(pick.spot);
        if pick.clicks >= 2 {
            self.browse_open(window, cx);
        } else {
            self.enter_browse(window, cx);
        }
    }

    fn browse_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(folder) = self.preview.folder_mut() {
            folder.step_by(delta);
            cx.notify();
        }
    }

    fn browse_fold(&mut self, is_opening: bool, cx: &mut Context<Self>) {
        let Some(fold) = self.preview.folder().and_then(|f| f.fold_for(is_opening)) else {
            return;
        };
        match fold {
            Fold::Toggle(index) => self.toggle_contents(index, cx),
            Fold::Step => self.browse_step(1, cx),
            Fold::Parent(index) => {
                if let Some(folder) = self.preview.folder_mut() {
                    folder.move_to_row(index);
                }
                cx.notify();
            }
        }
    }

    fn browse_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(place) = self.preview.folder().and_then(FolderPreview::place) else {
            return;
        };
        if !place.is_folder {
            self.browser.open_at(place.folder, place.name);
            self.focus_core_pane(window, cx);
            cx.notify();
        } else if let Some(location) = place.location() {
            self.apply(&Command::PaneNavigate { location }, window, cx);
        }
    }

    fn browse_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let place = self.preview.folder().and_then(FolderPreview::place);
        if let Some(location) = place.filter(|p| !p.is_folder).and_then(|p| p.location()) {
            self.view_location(location, window, cx);
        }
    }

    fn leave_browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    pub(super) fn browse_frame(&self, body: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("preview-browse")
            .key_context(BROWSE)
            .track_focus(&self.preview.browse)
            .on_any_mouse_down(|_, window, _| window.prevent_default())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .on_action(cx.listener(|this, _: &BrowseUp, _, cx| this.browse_step(-1, cx)))
            .on_action(cx.listener(|this, _: &BrowseDown, _, cx| this.browse_step(1, cx)))
            .on_action(cx.listener(|this, _: &BrowseFold, _, cx| this.browse_fold(false, cx)))
            .on_action(cx.listener(|this, _: &BrowseUnfold, _, cx| this.browse_fold(true, cx)))
            .on_action(cx.listener(|this, _: &BrowseOpen, window, cx| {
                this.browse_open(window, cx);
            }))
            .on_action(cx.listener(|this, _: &BrowseView, window, cx| {
                this.browse_view(window, cx);
            }))
            .on_action(cx.listener(|this, _: &LeaveBrowse, window, cx| {
                this.leave_browse(window, cx);
            }))
            .on_action(cx.listener(|this, _: &LeaveAndSwitch, window, cx| {
                this.leave_browse(window, cx);
                this.switch_pane(window, cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePreview, window, cx| {
                this.toggle_preview(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Recount, _, cx| this.recount_folder(cx)))
            .child(body)
            .into_any_element()
    }
}
