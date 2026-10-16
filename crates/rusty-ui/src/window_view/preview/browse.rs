use gpui_kit::{FocusHandle, KeyBinding};
use rusty_core::{Command, Location, ProviderPath};

use super::contents::Place;
use super::folder::FolderPreview;
use super::summary::LARGEST_AT;
use super::{Preview, Shown};
use crate::prelude::*;
use crate::window_view::FerriteWindow;
use crate::window_view::keys::TogglePreview;

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
        LeaveAndSwitch
    ]
);

pub(in crate::window_view) fn bind_keys(cx: &mut App) {
    let context = Some(BROWSE);
    cx.bind_keys([
        KeyBinding::new("up", BrowseUp, context),
        KeyBinding::new("down", BrowseDown, context),
        KeyBinding::new("left", BrowseFold, context),
        KeyBinding::new("right", BrowseUnfold, context),
        KeyBinding::new("enter", BrowseOpen, context),
        KeyBinding::new("f3", BrowseView, context),
        KeyBinding::new("escape", LeaveBrowse, context),
        KeyBinding::new("tab", LeaveAndSwitch, context),
        KeyBinding::new("ctrl-q", TogglePreview, context),
    ]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) enum Spot {
    Tree(usize),
    Largest(usize),
}

fn step(spot: Option<Spot>, delta: isize, (tree, largest): (usize, usize)) -> Option<Spot> {
    let last = (tree + largest).checked_sub(1)?;
    let at = match spot {
        None => 0,
        Some(Spot::Tree(index)) => index.saturating_add_signed(delta),
        Some(Spot::Largest(index)) => (tree + index).saturating_add_signed(delta),
    }
    .min(last);
    Some(if at < tree {
        Spot::Tree(at)
    } else {
        Spot::Largest(at - tree)
    })
}

fn largest_place(base: &Location, path: &str) -> Option<Place> {
    let mut parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let name = parts.pop()?.to_owned();
    let folder = parts.into_iter().try_fold(base.clone(), |at, part| {
        at.join(&ProviderPath::from_text(part)).ok()
    })?;
    Some(Place {
        folder,
        name,
        is_folder: false,
    })
}

impl Place {
    fn location(&self) -> Option<Location> {
        self.folder.join(&ProviderPath::from_text(&self.name)).ok()
    }
}

enum Fold {
    Toggle(usize),
    Step,
    Parent(usize),
}

impl FolderPreview {
    fn lens(&self) -> (usize, usize) {
        let largest = self.scan.profile.as_ref().map_or(0, |p| p.largest.len());
        (self.contents.len(), largest)
    }

    fn move_to(&mut self, spot: Option<Spot>) {
        self.spot = spot;
        match spot {
            Some(Spot::Tree(index)) => self.tree_scroll.scroll_to(index),
            Some(Spot::Largest(_)) => self.summary_scroll.scroll_to_item(LARGEST_AT),
            None => {}
        }
    }

    fn place(&self) -> Option<Place> {
        match self.spot? {
            Spot::Tree(index) => self.contents.place(index),
            Spot::Largest(index) => {
                let file = self.scan.profile.as_ref()?.largest.get(index)?;
                largest_place(&self.target.location, &file.path)
            }
        }
    }

    fn fold_for(&self, is_opening: bool) -> Option<Fold> {
        let Some(Spot::Tree(index)) = self.spot else {
            return None;
        };
        let (_, shape) = self.contents.row(index)?;
        match (is_opening, shape.can_open, shape.is_open) {
            (true, true, false) | (false, true, true) => Some(Fold::Toggle(index)),
            (true, true, true) => Some(Fold::Step),
            (false, _, _) => self.contents.parent_row(index).map(Fold::Parent),
            (true, false, _) => None,
        }
    }
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
        if let Some(folder) = self.preview.folder_mut()
            && folder.spot.is_none()
        {
            folder.spot = step(None, 0, folder.lens());
        }
        let target = self.preview.browse_target(cx);
        window.focus(&target, cx);
        cx.notify();
    }

    pub(super) fn pick_spot(
        &mut self,
        (spot, clicks): (Spot, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(folder) = self.preview.folder_mut() else {
            return;
        };
        folder.spot = Some(spot);
        if clicks >= 2 {
            self.browse_open(window, cx);
        } else {
            self.enter_browse(window, cx);
        }
    }

    fn browse_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(folder) = self.preview.folder_mut() {
            folder.move_to(step(folder.spot, delta, folder.lens()));
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
                    folder.move_to(Some(Spot::Tree(index)));
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
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Spot, step};

    #[test]
    fn a_step_crosses_from_the_tree_into_the_largest_rows_and_stops_at_both_ends() {
        assert_eq!(step(None, 1, (3, 2)), Some(Spot::Tree(0)));
        assert_eq!(step(Some(Spot::Tree(2)), 1, (3, 2)), Some(Spot::Largest(0)));
        assert_eq!(
            step(Some(Spot::Largest(0)), -1, (3, 2)),
            Some(Spot::Tree(2))
        );
        assert_eq!(
            step(Some(Spot::Largest(1)), 1, (3, 2)),
            Some(Spot::Largest(1))
        );
        assert_eq!(step(Some(Spot::Tree(0)), -1, (3, 2)), Some(Spot::Tree(0)));
        assert_eq!(step(Some(Spot::Tree(0)), 1, (0, 2)), Some(Spot::Largest(1)));
        assert_eq!(step(None, 1, (0, 0)), None);
    }
}
