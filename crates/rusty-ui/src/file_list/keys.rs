use std::rc::Rc;

use gpui_kit::{Action, App, Div, InteractiveElement, KeyBinding, Stateful, Window};
use rusty_core::{Command, CursorMove};

use super::{FileListAction, ListContext};

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

pub(super) fn register(root: Stateful<Div>, ctx: &Rc<ListContext>) -> Stateful<Div> {
    let open = ctx.clone();
    let up = ctx.clone();
    root.on_action(move_to::<CursorUp>(ctx, CursorMove::Up))
        .on_action(move_to::<CursorDown>(ctx, CursorMove::Down))
        .on_action(move_to::<PageUp>(ctx, CursorMove::PageUp))
        .on_action(move_to::<PageDown>(ctx, CursorMove::PageDown))
        .on_action(move_to::<CursorFirst>(ctx, CursorMove::First))
        .on_action(move_to::<CursorLast>(ctx, CursorMove::Last))
        .on_action(send::<ToggleMark>(ctx, Command::SelectionToggle))
        .on_action(send::<ToggleMarkDown>(ctx, Command::SelectionToggleDown))
        .on_action(move |_: &OpenRow, window, cx| open.open_cursor(window, cx))
        .on_action(move |_: &OpenParent, window, cx| up.act(FileListAction::Up, window, cx))
}
