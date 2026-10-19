use std::any::Any;
use std::rc::Rc;

use gpui_kit::{Action, AnyView, FocusHandle};
use rusty_core::Location;
use rusty_core::marks::{Bookmarks, MarkSet};

use super::FileHooks;
use super::parts::PartSet;
use crate::prelude::*;
use crate::settings::Values;

pub type Key = (&'static str, &'static str);
pub type OpenFile = Rc<dyn Fn(Location, &mut App)>;

pub enum BodyKeys {
    Modal(Vec<Key>),
    Normal(Vec<Key>),
}

#[derive(Clone)]
pub struct Host {
    pub title: SharedString,
    pub location: Location,
    pub hooks: FileHooks,
    pub on_saved: Rc<dyn Fn(&mut App)>,
    pub open: OpenFile,
    pub carried: Option<Carried>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Carried {
    pub marks: MarkSet,
    pub bookmarks: Option<Bookmarks>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyAct {
    Mark(u8),
    ClearMarks,
    ToggleBookmark,
    StepBookmark { backward: bool },
    LineEnds,
    Follow,
}

pub enum MenuEntry {
    Run(&'static str, Box<dyn Action>, bool),
    Submenu(&'static str, Vec<MenuEntry>),
    Break,
}

pub struct Attach {
    pub loaded: Box<dyn Any + Send>,
    pub host: Host,
    pub edit: bool,
}

pub trait ViewerBody: Any {
    fn view(&self) -> AnyView;
    fn focus_handle(&self, cx: &App) -> FocusHandle;
    fn keys(&self, cx: &App) -> BodyKeys;
    fn find_again(&self, backward: bool, window: &mut Window, cx: &mut App);
    fn set_note(&self, note: SharedString, cx: &mut App);
    fn as_any(&self) -> &dyn Any;

    fn is_editing(&self, _: &App) -> bool {
        false
    }

    fn is_typing(&self, _: &App) -> bool {
        false
    }

    fn can_start_editing(&self, _: &App) -> bool {
        false
    }

    fn start_editing(&self, _: &mut App) -> bool {
        false
    }

    fn is_dirty(&self, _: &App) -> bool {
        false
    }

    fn revision(&self, _: &App) -> u64 {
        0
    }

    fn save(&self, _: &mut App) {}

    fn act(&self, _: BodyAct, _: &mut Window, _: &mut App) {}

    fn go_to_line(&self, _line: u64, _: &mut Window, _: &mut App) {}

    fn carry(&self, _: &App) -> Option<Carried> {
        None
    }

    fn menu(&self, _by_pointer: bool, _: &mut App) -> Vec<MenuEntry> {
        Vec::new()
    }

    fn menu_closed(&self, _: &mut App) {}

    fn menu_anchor(&self, _: &App) -> Option<Point<Pixels>> {
        None
    }

    fn parts(&self, _: &App) -> Option<&'static PartSet> {
        None
    }

    fn show_parts(&self, _: &Values, _: &mut App) {}
}
