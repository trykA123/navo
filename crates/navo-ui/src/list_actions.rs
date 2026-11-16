//! The list action and key context that kit-side code shares with the file list.

gpui_kit::actions!(file_list, [UndoLast]);

pub const LIST_CONTEXT: &str = "FileList";
