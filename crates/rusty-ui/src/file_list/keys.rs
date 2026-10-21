use std::rc::Rc;

use gpui_kit::{
    Action, App, Div, InteractiveElement, MouseButton, NavigationDirection, Stateful, Window,
};
use rusty_core::{Command, CursorMove, OpKind, SpanMode};

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
        ClipboardCopy,
        ClipboardCut,
        Paste,
        RecycleItems,
        DeleteItems,
        GoBack,
        GoForward,
        ClearFilter,
        RenameItem,
        MultiRename,
        UndoLast,
        NewFolder,
        NewFile,
        CopyPath,
        MarkPattern,
        UnmarkPattern,
        InvertMarks,
        ExtendUp,
        ExtendDown,
        ExtendPageUp,
        ExtendPageDown,
        ExtendFirst,
        ExtendLast,
        MarkAll,
        UnmarkAll,
        RestoreMarks,
        MarkExtension,
        UnmarkExtension,
        SortName,
        SortExt,
        SortDate,
        SortSize,
        EditItem,
        ChooseEditor,
        ViewItem,
        Refresh,
        OpenMenu,
    ]
);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "file_list");
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

fn extend<A: Action>(
    ctx: &Rc<ListContext>,
    to: CursorMove,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let mode = SpanMode::Mark;
    send(ctx, Command::SelectionExtend { to, mode })
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

fn list_act<A: Action>(
    ctx: &Rc<ListContext>,
    action: FileListAction,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |_, window, cx| ctx.act(action, window, cx)
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
    register_marks(register_clipboard(register_more(root, ctx), ctx), ctx)
}

fn register_marks(root: Stateful<Div>, ctx: &Rc<ListContext>) -> Stateful<Div> {
    root.on_action(extend::<ExtendUp>(ctx, CursorMove::Up))
        .on_action(extend::<ExtendDown>(ctx, CursorMove::Down))
        .on_action(extend::<ExtendPageUp>(ctx, CursorMove::PageUp))
        .on_action(extend::<ExtendPageDown>(ctx, CursorMove::PageDown))
        .on_action(extend::<ExtendFirst>(ctx, CursorMove::First))
        .on_action(extend::<ExtendLast>(ctx, CursorMove::Last))
        .on_action(send::<MarkAll>(ctx, Command::SelectionAll))
        .on_action(send::<UnmarkAll>(ctx, Command::SelectionClear))
        .on_action(send::<RestoreMarks>(ctx, Command::SelectionRestore))
        .on_action(send::<MarkExtension>(
            ctx,
            Command::SelectionExtension { mark: true },
        ))
        .on_action(send::<UnmarkExtension>(
            ctx,
            Command::SelectionExtension { mark: false },
        ))
}

fn register_clipboard(root: Stateful<Div>, ctx: &Rc<ListContext>) -> Stateful<Div> {
    root.on_action(list_act::<ClipboardCopy>(
        ctx,
        FileListAction::CopyToClipboard,
    ))
    .on_action(list_act::<ClipboardCut>(
        ctx,
        FileListAction::CutToClipboard,
    ))
    .on_action(list_act::<Paste>(ctx, FileListAction::Paste))
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
    .on_action(list_act::<NewFile>(ctx, FileListAction::NewFile))
    .on_action(list_act::<CopyPath>(ctx, FileListAction::CopyPath))
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
    .on_action({
        let ctx = ctx.clone();
        move |_: &OpenMenu, window, cx| ctx.menu_at_cursor(window, cx)
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
    .on_action(send::<Refresh>(ctx, Command::PaneRefresh))
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
