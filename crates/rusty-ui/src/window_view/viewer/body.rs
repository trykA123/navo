use std::any::Any;
use std::rc::Rc;

use gpui_kit::{AnyView, FocusHandle};
use rusty_core::Location;

use super::FileHooks;
use crate::prelude::*;

pub type Key = (&'static str, &'static str);

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
}
