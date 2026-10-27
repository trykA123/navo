use gpui_kit::ClipboardItem;
use rusty_core::{FsError, FsErrorKind, Location};
use rusty_fs::FsEntry;
use rusty_l10n::t;

use super::super::FerriteWindow;
use super::super::hand_off::look_up;
use super::super::path_edit::{is_folder, last_name};
use crate::command_center::{Row, Target};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Way {
    Open,
    Reveal,
    OtherPane,
    NewTab,
    CopyPath,
}

impl Way {
    pub(super) fn of_click(modifiers: Modifiers) -> Self {
        if modifiers.alt {
            Self::NewTab
        } else if modifiers.control {
            Self::OtherPane
        } else if modifiers.shift {
            Self::Reveal
        } else {
            Self::Open
        }
    }

    pub(super) fn runs_commands(self) -> bool {
        self == Self::Open
    }
}

pub(super) struct Place {
    pub(super) location: Location,
    pub(super) folder: bool,
}

type Found = Option<Result<FsEntry, FsError>>;

fn split(location: &Location) -> Option<(Location, String)> {
    Some((location.parent()?, last_name(location)?))
}

fn is_location(row: Option<&Row>) -> bool {
    matches!(
        row,
        Some(Row::TopHit(hit) | Row::Hit(hit))
            if matches!(hit.target, Target::Open { .. } | Target::GoTo(_))
    )
}

impl FerriteWindow {
    pub(super) fn click_way(&self, modifiers: Modifiers) -> Way {
        let Some(open) = self.command_center.open.as_ref() else {
            return Way::Open;
        };
        if open.step.is_some() || !is_location(open.selection.row(&open.rows)) {
            return Way::Open;
        }
        Way::of_click(modifiers)
    }

    pub(super) fn open_place(
        &mut self,
        (place, way): (Place, Way),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if way == Way::CopyPath {
            let text = place.location.as_str().to_owned();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            return self.close_command_center(window, cx);
        }
        self.close_command_center(window, cx);
        let provider = self.provider.clone();
        cx.spawn_in(window, async move |this, cx| {
            let found = look_up(provider, place.location.clone(), cx).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.open_found((place, way, found), window, cx);
            });
        })
        .detach();
    }

    fn open_found(
        &mut self,
        (place, way, found): (Place, Way, Found),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Place { location, folder } = place;
        if self.prompt.is_some() || self.browser.pending().is_some() || self.is_multi_rename_open()
        {
            return;
        }
        let folder = match found {
            Some(Err(error)) if error.kind == FsErrorKind::NotFound => {
                let path = location.as_str().to_owned();
                return self.show_notice(StatusNotice {
                    text: t!("command-center-gone", path = path).into(),
                    is_error: true,
                });
            }
            Some(Ok(entry)) => is_folder(&entry),
            _ => folder,
        };
        if way == Way::OtherPane {
            let result = self.browser.focus_target_pane();
            self.show_refusal(result);
        }
        let opened = self.open_way(&location, folder, way);
        if opened {
            if !folder && way != Way::Open {
                self.note_file(&location, cx);
            }
            self.note_seen(cx);
            self.focus_core_pane(window, cx);
        }
        cx.notify();
    }

    fn open_way(&mut self, location: &Location, folder: bool, way: Way) -> bool {
        let reveal = way == Way::Reveal || (way == Way::OtherPane && !folder);
        match (way, folder) {
            (Way::NewTab, true) => self.browser.open_tab_at(location.clone(), None),
            (Way::NewTab, false) => split(location)
                .is_some_and(|(parent, name)| self.browser.open_tab_at(parent, Some(name))),
            _ if reveal => match split(location) {
                Some((parent, name)) => self.browser.open_at(parent, name),
                None => folder && self.browser.open_location(location.clone(), true),
            },
            _ => self.browser.open_location(location.clone(), folder),
        }
    }
}
