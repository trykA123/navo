use gpui_kit::{Pixels, Point};
use rusty_core::OpKind;

use super::SortBy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterEdit {
    Push(char),
    Pop,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileListAction {
    Open(u64),
    Up,
    SwitchPane,
    Operate(OpKind),
    CopyToClipboard,
    CutToClipboard,
    Paste,
    Filter(FilterEdit),
    Rename(u64),
    NewFolder,
    NewFile,
    SelectPattern { add: bool },
    Sort(SortBy),
    Edit { row: u64, choose: bool },
    View(u64),
    ContextMenu(Point<Pixels>),
    BackgroundMenu(Point<Pixels>),
    MenuPrefetch { adds_cursor: bool },
    MenuCancel,
}
