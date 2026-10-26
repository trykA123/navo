use std::path::{Path, PathBuf};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use rusty_core::{DropKeys, DropOp, Location};

use crate::prelude::*;

pub fn invalid_drop_path(path: &Path) -> String {
    rusty_l10n::t!("drag-refused-path", path = path.display().to_string())
}

pub trait DropPlatform {
    fn keys(&self, _window: Option<isize>) -> Option<DropKeys> {
        None
    }

    fn right_button(&self, _window: Option<isize>) -> bool {
        false
    }

    fn allows(&self, _window: Option<isize>, _op: DropOp) -> bool {
        true
    }

    fn set_effect(&self, _window: Option<isize>, _op: Option<DropOp>) {}

    fn locations(&self, paths: &[PathBuf]) -> Result<Vec<Location>, String>;

    fn paths(&self, items: &[(Location, bool)]) -> Option<Vec<(PathBuf, bool)>>;
}

pub(super) fn native_handle(window: &Window) -> Option<isize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

pub(super) struct DragDrop {
    pub(super) session: Option<super::drop::DragSession>,
    pub(super) platform: Option<std::rc::Rc<dyn DropPlatform>>,
    pub(super) menu: Option<super::drop_menu::DropMenu>,
}

impl DragDrop {
    pub(super) fn new(platform: Option<std::rc::Rc<dyn DropPlatform>>) -> Self {
        Self {
            session: None,
            platform,
            menu: None,
        }
    }
}
