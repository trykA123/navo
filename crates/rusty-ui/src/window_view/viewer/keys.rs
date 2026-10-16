use gpui_kit::KeyBinding;

use super::search::{FIND_ALL_CONTEXT, FIND_CONTEXT};
use super::{CONTEXT, IMAGE_CONTEXT, PAGED_CONTEXT};
use crate::prelude::*;

gpui_kit::actions!(
    viewer,
    [
        SaveFile,
        CloseViewer,
        EditFile,
        ShowText,
        ShowHex,
        ShowRich,
        ListerText,
        ListerHex,
        ListerRich,
        FindNext,
        FindOnEnter,
        FindPrevious,
        NextFile,
        PreviousFile,
        RowUp,
        RowDown,
        PageUp,
        PageDown,
        GoStart,
        GoEnd,
        ScrollLeft,
        ScrollRight,
        OpenFind,
        OpenGoTo,
        MatchCase,
        Back,
        Mark1,
        Mark2,
        Mark3,
        Mark4,
        Mark5,
        Mark6,
        Mark7,
        Mark8,
        ClearMarks,
        ToggleBookmark,
        NextBookmark,
        PreviousBookmark,
        ShowLineEnds,
        ToggleFollow,
        OlderSearch,
        NewerSearch,
        FindAll,
        ListUp,
        ListDown,
        ListGo,
        ListClose,
        FitImage,
        ActualSize,
        ZoomIn,
        ZoomOut,
        ToggleInfo,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("ctrl-s", SaveFile, context),
        KeyBinding::new("escape", CloseViewer, context),
        KeyBinding::new("ctrl-e", EditFile, context),
        KeyBinding::new("alt-1", ShowText, context),
        KeyBinding::new("alt-3", ShowHex, context),
        KeyBinding::new("alt-5", ShowRich, context),
        KeyBinding::new("1", ListerText, context),
        KeyBinding::new("3", ListerHex, context),
        KeyBinding::new("5", ListerRich, context),
        KeyBinding::new("f3", FindNext, context),
        KeyBinding::new("shift-f3", FindPrevious, context),
        KeyBinding::new("enter", FindOnEnter, context),
        KeyBinding::new("n", NextFile, context),
        KeyBinding::new("p", PreviousFile, context),
        KeyBinding::new("space", NextFile, context),
        KeyBinding::new("backspace", PreviousFile, context),
        KeyBinding::new("up", RowUp, context),
        KeyBinding::new("down", RowDown, context),
        KeyBinding::new("pageup", PageUp, context),
        KeyBinding::new("pagedown", PageDown, context),
        KeyBinding::new("ctrl-home", GoStart, context),
        KeyBinding::new("ctrl-end", GoEnd, context),
        KeyBinding::new("left", ScrollLeft, context),
        KeyBinding::new("right", ScrollRight, context),
        KeyBinding::new("ctrl-f", OpenFind, context),
        KeyBinding::new("ctrl-g", OpenGoTo, context),
        KeyBinding::new("alt-c", MatchCase, context),
        KeyBinding::new("escape", Back, Some(PAGED_CONTEXT)),
    ]);
    bind_marking_keys(cx);
    bind_image_keys(cx);
}

fn bind_image_keys(cx: &mut App) {
    let context = Some(IMAGE_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("0", FitImage, context),
        KeyBinding::new("ctrl-0", FitImage, context),
        KeyBinding::new("1", ActualSize, context),
        KeyBinding::new("+", ZoomIn, context),
        KeyBinding::new("=", ZoomIn, context),
        KeyBinding::new("shift-=", ZoomIn, context),
        KeyBinding::new("-", ZoomOut, context),
        KeyBinding::new("i", ToggleInfo, context),
    ]);
}

fn bind_marking_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    let history = format!("{FIND_CONTEXT} > Input");
    cx.bind_keys([
        KeyBinding::new("ctrl-1", Mark1, context),
        KeyBinding::new("ctrl-2", Mark2, context),
        KeyBinding::new("ctrl-3", Mark3, context),
        KeyBinding::new("ctrl-4", Mark4, context),
        KeyBinding::new("ctrl-5", Mark5, context),
        KeyBinding::new("ctrl-6", Mark6, context),
        KeyBinding::new("ctrl-7", Mark7, context),
        KeyBinding::new("ctrl-8", Mark8, context),
        KeyBinding::new("ctrl-0", ClearMarks, context),
        KeyBinding::new("ctrl-f2", ToggleBookmark, context),
        KeyBinding::new("f2", NextBookmark, context),
        KeyBinding::new("shift-f2", PreviousBookmark, context),
        KeyBinding::new("alt-l", ShowLineEnds, context),
        KeyBinding::new("alt-f", ToggleFollow, context),
        KeyBinding::new("up", OlderSearch, Some(history.as_str())),
        KeyBinding::new("down", NewerSearch, Some(history.as_str())),
        KeyBinding::new("alt-enter", FindAll, Some(FIND_CONTEXT)),
        KeyBinding::new("escape", Back, Some(FIND_CONTEXT)),
        KeyBinding::new("up", ListUp, Some(FIND_ALL_CONTEXT)),
        KeyBinding::new("down", ListDown, Some(FIND_ALL_CONTEXT)),
        KeyBinding::new("enter", ListGo, Some(FIND_ALL_CONTEXT)),
        KeyBinding::new("escape", ListClose, Some(FIND_ALL_CONTEXT)),
    ]);
}
