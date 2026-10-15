use std::rc::Rc;

use rusty_core::{
    AppState, Command, CursorMove, Effect, Event, Location, ProviderId, Selection, apply,
    apply_event,
};
use rusty_fs::{EntryKind, FsEntry};
use rusty_ui::FocusHandle;
use rusty_ui::file_list::{FileList, FileListAction, FileListProps};
use rusty_ui::fkey_bar::FKeyBar;
use rusty_ui::pane_chrome::{PaneTabs, PathBar, path_segments};
use rusty_ui::prelude::*;
use rusty_ui::status_bar::{StatusBar, StatusBarProps};
use rusty_ui::tokens;

use crate::pane_sample::{self, FKEYS};

const FREE_BYTES: u64 = 512_110_190_592;

pub struct PanePage {
    state: AppState,
    rows: Rc<[FsEntry]>,
    focus: FocusHandle,
}

fn location(text: &str) -> Location {
    Location::new(text).unwrap_or_else(|_| Location::provider_root(ProviderId::local()))
}

impl PanePage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            state: AppState::new(location(r"D:\work\commander")),
            rows: pane_sample::rows(),
            focus: cx.focus_handle(),
        };
        let core = location(r"D:\work\commander\crates\core");
        page.run(&Command::PaneRefresh);
        page.run(&Command::PaneOpenTab {
            location: Some(core),
        });
        for row in [7, 9, 4] {
            page.run(&Command::PaneCursorMove {
                to: CursorMove::Row(row),
            });
            if row != 4 {
                page.run(&Command::SelectionToggle);
            }
        }
        page
    }

    fn run(&mut self, command: &Command) {
        let effects = apply(&mut self.state, command).unwrap_or_default();
        for effect in effects {
            if let Effect::ListFolder {
                tab,
                location,
                request,
            } = effect
            {
                let listing = pane_sample::listing(&self.rows);
                apply_event(
                    &mut self.state,
                    Event::ListingLoaded {
                        tab,
                        location,
                        request,
                        listing,
                    },
                );
            }
        }
    }

    fn open(&mut self, action: FileListAction) {
        let tab = self.state.active_tab();
        let target = match action {
            FileListAction::Up => tab.location().parent(),
            FileListAction::Open(row) => usize::try_from(row)
                .ok()
                .and_then(|index| self.rows.get(index))
                .and_then(|entry| tab.location().join(&entry.name).ok()),
            FileListAction::SwitchPane
            | FileListAction::Operate(_)
            | FileListAction::Filter(_)
            | FileListAction::Rename(_)
            | FileListAction::NewFolder
            | FileListAction::SelectPattern { .. }
            | FileListAction::Sort(_)
            | FileListAction::Edit { .. }
            | FileListAction::View(_) => None,
        };
        if let Some(location) = target {
            self.run(&Command::PaneNavigate { location });
        }
    }

    fn set_page_rows(&mut self, rows: u64) {
        let pane = self.state.focused_pane();
        apply_event(&mut self.state, Event::PageRowsChanged { pane, rows });
    }

    fn list_props(&self) -> FileListProps {
        let tab = self.state.active_tab();
        let folder = path_segments(tab.location())
            .last()
            .map(|segment| segment.label.clone())
            .unwrap_or_default();
        FileListProps {
            folder,
            rows: self.rows.clone(),
            view: tab.view(),
            marked: Rc::new(tab.selection().clone()),
            is_active: true,
            filter: None,
            sort: rusty_ui::file_list::SortKey::default(),
        }
    }

    fn status(&self) -> StatusBarProps {
        status_of(&self.rows, self.state.active_tab().selection())
    }

    fn tab_titles(&self) -> (Vec<SharedString>, usize) {
        let pane = self.state.pane(self.state.focused_pane());
        let titles = pane
            .map(|pane| {
                pane.tabs()
                    .iter()
                    .filter_map(|tab| path_segments(tab.location()).pop())
                    .map(|segment| segment.label)
                    .collect()
            })
            .unwrap_or_default();
        (titles, pane.map_or(0, |pane| pane.active_index()))
    }
}

fn status_of(rows: &[FsEntry], marked: &Selection) -> StatusBarProps {
    let mut status = StatusBarProps {
        free_bytes: Some(FREE_BYTES),
        ..StatusBarProps::default()
    };
    for (row, entry) in (0..).zip(rows) {
        let is_folder = entry.kind == EntryKind::Folder;
        status.folders += u64::from(is_folder);
        status.files += u64::from(!is_folder);
        status.bytes += entry.size;
        if marked.is_marked(row) {
            status.marked += 1;
            status.marked_bytes += entry.size;
        }
    }
    status
}

impl PanePage {
    fn file_list(&self, cx: &mut Context<Self>) -> FileList {
        FileList::new("pane-list", self.list_props())
            .track_focus(&self.focus)
            .on_command(cx.listener(|this: &mut Self, command: &Command, _, cx| {
                this.run(command);
                cx.notify();
            }))
            .on_list_action(
                cx.listener(|this: &mut Self, action: &FileListAction, _, cx| {
                    this.open(*action);
                    cx.notify();
                }),
            )
            .on_page_rows(
                cx.listener(|this: &mut Self, rows: &u64, _, _| this.set_page_rows(*rows)),
            )
    }

    fn path_bar(&self, cx: &mut Context<Self>) -> PathBar {
        let location = self.state.active_tab().location().clone();
        PathBar::new("pane-path", location)
            .active(true)
            .on_navigate(cx.listener(|this: &mut Self, location: &Location, _, cx| {
                this.run(&Command::PaneNavigate {
                    location: location.clone(),
                });
                cx.notify();
            }))
    }
}

impl Render for PanePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let (titles, active) = self.tab_titles();
        let tabs = PaneTabs::new("pane-tabs", titles, active).on_command(cx.listener(
            |this: &mut Self, command: &Command, _, cx| {
                this.run(command);
                cx.notify();
            },
        ));
        let pane = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(tabs)
            .child(self.path_bar(cx))
            .child(self.file_list(cx))
            .child(StatusBar::new("pane-status", self.status()));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface)
            .child(pane)
            .child(FKeyBar::new("pane-fkeys", FKEYS))
    }
}
