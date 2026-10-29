use std::rc::Rc;
use std::sync::Arc;

use rusty_core::{Location, ProviderId, Selection, ViewState};
use rusty_fs::FsEntry;
use rusty_ui::FocusHandle;
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::pane_chrome::{
    DriveIcon, DriveInfo, DriveMenu, DrivePill, PathBar, PathInput, current_drive,
};
use rusty_ui::prelude::*;
use rusty_ui::status_bar::{StatusBar, StatusBarProps};
use rusty_ui::tokens;

use crate::pane_sample;

const GIB: u64 = 1 << 30;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shows {
    Menu,
    PathError,
}

pub struct DrivesPage {
    shows: Shows,
    rows: Arc<[FsEntry]>,
    focus: [FocusHandle; 2],
    menu_focus: FocusHandle,
}

fn location(text: &str) -> Location {
    Location::new(text).unwrap_or_else(|_| Location::provider_root(ProviderId::local()))
}

struct Sample {
    letter: char,
    icon: DriveIcon,
    label: &'static str,
    remote: Option<&'static str>,
    connected: bool,
    space: Option<(u64, u64)>,
}

fn drive(sample: Sample) -> DriveInfo {
    DriveInfo {
        root: location(&format!("{}:\\", sample.letter)),
        letter: Some(sample.letter),
        icon: sample.icon,
        label: Some(sample.label.to_owned()),
        remote: sample.remote.map(str::to_owned),
        connected: sample.connected,
        space: sample.space,
    }
}

fn sample(letter: char, icon: DriveIcon, label: &'static str, free_of_100: u64) -> Sample {
    Sample {
        letter,
        icon,
        label,
        remote: None,
        connected: true,
        space: Some((100 * GIB * 4, free_of_100 * GIB * 4)),
    }
}

pub fn six_drives() -> Rc<[DriveInfo]> {
    let network = Sample {
        remote: Some(r"\\nas\media"),
        ..sample('Z', DriveIcon::Network, "media", 58)
    };
    let gone = Sample {
        remote: Some(r"\\nas\backup"),
        connected: false,
        space: None,
        ..sample('Y', DriveIcon::Network, "backup", 0)
    };
    let optical = Sample {
        space: None,
        ..sample('R', DriveIcon::Optical, "Photos 2019", 0)
    };
    [
        sample('C', DriveIcon::Fixed, "System", 65),
        sample('D', DriveIcon::Fixed, "Work", 7),
        sample('E', DriveIcon::Removable, "Stick", 81),
        network,
        gone,
        optical,
    ]
    .into_iter()
    .map(drive)
    .collect()
}

fn two_drives() -> Rc<[DriveInfo]> {
    six_drives().iter().take(2).cloned().collect()
}

fn twelve_drives() -> Rc<[DriveInfo]> {
    let more = "FGHIJK".chars().map(|letter| {
        drive(Sample {
            space: Some((GIB * 500, GIB * 120)),
            ..sample(letter, DriveIcon::Fixed, "Archive", 0)
        })
    });
    six_drives().iter().cloned().chain(more).collect()
}

impl DrivesPage {
    pub fn new(shows: Shows, cx: &mut Context<Self>) -> Self {
        Self {
            shows,
            rows: pane_sample::rows(),
            focus: [cx.focus_handle(), cx.focus_handle()],
            menu_focus: cx.focus_handle(),
        }
    }

    fn list(&self, side: usize, folder: &str) -> FileList {
        let props = FileListProps {
            folder: folder.to_owned().into(),
            rows: self.rows.clone(),
            view: ViewState {
                cursor: 4,
                scroll: 0,
            },
            marked: Rc::new(Selection::new(self.rows.len() as u64)),
            is_active: side == 0,
            filter: None,
            sort: SortKey::default(),
        };
        FileList::new(("drives-list", side), props).track_focus(&self.focus[side])
    }

    fn pane(&self, side: usize, drives: Rc<[DriveInfo]>, path: &str) -> Div {
        let here = location(path);
        let current = current_drive(&drives, &here);
        let is_open = side == 0 && self.shows == Shows::Menu;
        let drive = current.and_then(|index| drives.get(index).cloned());
        let pill = div()
            .relative()
            .child(DrivePill::new(("drive-pill", side), drive).open(is_open))
            .when(is_open, |d| {
                let menu = DriveMenu::new(("drive-menu", side), drives, 1)
                    .current(current)
                    .track_focus(&self.menu_focus);
                d.child(menu.into_popover(px(0.)))
            });
        let bar = div().flex().justify_end().px(px(8.)).py(px(6.)).child(pill);
        let path_row = if side == 0 && self.shows == Shows::PathError {
            PathInput::new(("path-input", side), r"%PROJECTS%\commander")
                .error(Some("Unknown variable %PROJECTS%.".into()))
                .into_any_element()
        } else {
            PathBar::new(("path", side), here.clone()).into_any_element()
        };
        let status = StatusBarProps {
            files: 11,
            folders: 2,
            ..StatusBarProps::default()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(bar)
            .child(path_row)
            .child(self.list(side, "commander"))
            .child(StatusBar::new(("drives-status", side), status))
    }
}

impl Render for DrivesPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let (left, right) = match self.shows {
            Shows::Menu => (six_drives(), six_drives()),
            Shows::PathError => (two_drives(), twelve_drives()),
        };
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface)
            .child(self.pane(0, left, r"C:\work\commander"))
            .child(div().w(px(1.)).h_full().bg(t.colors.line))
            .child(self.pane(1, right, r"D:\backup\2026"))
    }
}
