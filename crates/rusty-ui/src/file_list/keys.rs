use std::rc::Rc;

use gpui_kit::{
    Action, App, Div, InteractiveElement, KeyBinding, MouseButton, NavigationDirection, Stateful,
    Window,
};
use rusty_core::{Command, CursorMove, OpKind};

use super::{FileListAction, ListContext, SortBy};

pub(super) const CONTEXT: &str = "FileList";

gpui_kit::actions!(
    file_list,
    [
        CursorUp,
        CursorDown,
        PageUp,
        PageDown,
        CursorFirst,
        CursorLast,
        ToggleMark,
        ToggleMarkDown,
        OpenRow,
        OpenParent,
        SwitchPane,
        CopyItems,
        MoveItems,
        RecycleItems,
        DeleteItems,
        GoBack,
        GoForward,
        ClearFilter,
        RenameItem,
        NewFolder,
        MarkPattern,
        UnmarkPattern,
        InvertMarks,
        SortName,
        SortExt,
        SortDate,
        SortSize,
        EditItem,
        ChooseEditor,
        ViewItem,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", CursorUp, context),
        KeyBinding::new("down", CursorDown, context),
        KeyBinding::new("pageup", PageUp, context),
        KeyBinding::new("pagedown", PageDown, context),
        KeyBinding::new("home", CursorFirst, context),
        KeyBinding::new("end", CursorLast, context),
        KeyBinding::new("space", ToggleMark, context),
        KeyBinding::new("insert", ToggleMarkDown, context),
        KeyBinding::new("enter", OpenRow, context),
        KeyBinding::new("backspace", OpenParent, context),
        KeyBinding::new("tab", SwitchPane, context),
        KeyBinding::new("f5", CopyItems, context),
        KeyBinding::new("f6", MoveItems, context),
        KeyBinding::new("f8", RecycleItems, context),
        KeyBinding::new("delete", RecycleItems, context),
        KeyBinding::new("shift-delete", DeleteItems, context),
        KeyBinding::new("shift-f8", DeleteItems, context),
        KeyBinding::new("alt-left", GoBack, context),
        KeyBinding::new("alt-right", GoForward, context),
        KeyBinding::new("escape", ClearFilter, context),
        KeyBinding::new("f2", RenameItem, context),
        KeyBinding::new("shift-f6", RenameItem, context),
        KeyBinding::new("f7", NewFolder, context),
        KeyBinding::new("+", MarkPattern, context),
        KeyBinding::new("-", UnmarkPattern, context),
        KeyBinding::new("*", InvertMarks, context),
        KeyBinding::new("ctrl-f3", SortName, context),
        KeyBinding::new("ctrl-f4", SortExt, context),
        KeyBinding::new("ctrl-f5", SortDate, context),
        KeyBinding::new("ctrl-f6", SortSize, context),
        KeyBinding::new("f3", ViewItem, context),
        KeyBinding::new("f4", EditItem, context),
        KeyBinding::new("shift-f4", ChooseEditor, context),
    ]);
}

fn send<A: Action>(
    ctx: &Rc<ListContext>,
    command: Command,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |_, window, cx| ctx.send(command.clone(), window, cx)
}

fn move_to<A: Action>(
    ctx: &Rc<ListContext>,
    to: CursorMove,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    send(ctx, Command::PaneCursorMove { to })
}

fn sort<A: Action>(
    ctx: &Rc<ListContext>,
    by: SortBy,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |_, window, cx| ctx.act(FileListAction::Sort(by), window, cx)
}

fn mark<A: Action>(
    ctx: &Rc<ListContext>,
    key: char,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |_, window, cx| ctx.mark_key(key, window, cx)
}

fn operate<A: Action>(
    ctx: &Rc<ListContext>,
    kind: OpKind,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |_, window, cx| ctx.act(FileListAction::Operate(kind), window, cx)
}

pub(super) fn register(root: Stateful<Div>, ctx: &Rc<ListContext>) -> Stateful<Div> {
    let open = ctx.clone();
    let up = ctx.clone();
    let switch = ctx.clone();
    let root = root
        .on_action(move_to::<CursorUp>(ctx, CursorMove::Up))
        .on_action(move_to::<CursorDown>(ctx, CursorMove::Down))
        .on_action(move_to::<PageUp>(ctx, CursorMove::PageUp))
        .on_action(move_to::<PageDown>(ctx, CursorMove::PageDown))
        .on_action(move_to::<CursorFirst>(ctx, CursorMove::First))
        .on_action(move_to::<CursorLast>(ctx, CursorMove::Last))
        .on_action(send::<ToggleMark>(ctx, Command::SelectionToggle))
        .on_action(send::<ToggleMarkDown>(ctx, Command::SelectionToggleDown))
        .on_action(move |_: &OpenRow, window, cx| open.open_cursor(window, cx))
        .on_action(move |_: &OpenParent, window, cx| up.go_up_or_narrow(window, cx))
        .on_action(move |_: &SwitchPane, window, cx| {
            switch.act(FileListAction::SwitchPane, window, cx);
        })
        .on_action(operate::<CopyItems>(ctx, OpKind::Copy))
        .on_action(operate::<MoveItems>(ctx, OpKind::Move))
        .on_action(operate::<RecycleItems>(ctx, OpKind::Recycle))
        .on_action(operate::<DeleteItems>(ctx, OpKind::Delete))
        .on_action({
            let ctx = ctx.clone();
            move |_: &ClearFilter, window, cx| ctx.clear_filter(window, cx)
        });
    register_more(root, ctx)
}

fn register_more(root: Stateful<Div>, ctx: &Rc<ListContext>) -> Stateful<Div> {
    root.on_action({
        let ctx = ctx.clone();
        move |_: &RenameItem, window, cx| ctx.rename_cursor(window, cx)
    })
    .on_action({
        let ctx = ctx.clone();
        move |_: &NewFolder, window, cx| ctx.act(FileListAction::NewFolder, window, cx)
    })
    .on_action({
        let ctx = ctx.clone();
        move |_: &ViewItem, window, cx| ctx.view_cursor(window, cx)
    })
    .on_action({
        let ctx = ctx.clone();
        move |_: &EditItem, window, cx| ctx.edit_cursor(false, window, cx)
    })
    .on_action({
        let ctx = ctx.clone();
        move |_: &ChooseEditor, window, cx| ctx.edit_cursor(true, window, cx)
    })
    .on_action(sort::<SortName>(ctx, SortBy::Name))
    .on_action(sort::<SortExt>(ctx, SortBy::Ext))
    .on_action(sort::<SortDate>(ctx, SortBy::Date))
    .on_action(sort::<SortSize>(ctx, SortBy::Size))
    .on_action(mark::<MarkPattern>(ctx, '+'))
    .on_action(mark::<UnmarkPattern>(ctx, '-'))
    .on_action(mark::<InvertMarks>(ctx, '*'))
    .on_key_down({
        let ctx = ctx.clone();
        move |event, window, cx| ctx.type_key(event, window, cx)
    })
    .on_action(send::<GoBack>(ctx, Command::PaneBack))
    .on_action(send::<GoForward>(ctx, Command::PaneForward))
    .on_mouse_down(side_button(NavigationDirection::Back), {
        let ctx = ctx.clone();
        move |_, window, cx| ctx.send(Command::PaneBack, window, cx)
    })
    .on_mouse_down(side_button(NavigationDirection::Forward), {
        let ctx = ctx.clone();
        move |_, window, cx| ctx.send(Command::PaneForward, window, cx)
    })
}

fn side_button(direction: NavigationDirection) -> MouseButton {
    MouseButton::Navigate(direction)
}
