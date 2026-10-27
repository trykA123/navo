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
    CopyPath,
    SelectPattern { add: bool },
    Sort(SortBy),
    ResizeColumn { by: SortBy, width: Pixels },
    Edit { row: u64, choose: bool },
    View(u64),
    ContextMenu { at: Point<Pixels>, extended: bool },
    BackgroundMenu { at: Point<Pixels>, extended: bool },
    MenuPrefetch { adds_cursor: bool, extended: bool },
    MenuCancel,
}
