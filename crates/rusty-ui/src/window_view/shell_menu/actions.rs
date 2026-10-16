use gpui_kit::{Action, ClipboardItem};
use rusty_core::Location;
use rusty_core::shell_menu::FerriteAction;

use super::FerriteWindow;
use super::run::ShellCall;
use crate::file_list::{
    CopyItems, EditItem, MoveItems, OpenRow, RecycleItems, RenameItem, ViewItem,
};
use crate::prelude::*;

impl FerriteWindow {
    pub(super) fn run_ferrite(
        &mut self,
        action: FerriteAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let list_action: Box<dyn Action> = match action {
            FerriteAction::Open => Box::new(OpenRow),
            FerriteAction::View => Box::new(ViewItem),
            FerriteAction::Edit => Box::new(EditItem),
            FerriteAction::Copy => Box::new(CopyItems),
            FerriteAction::Move => Box::new(MoveItems),
            FerriteAction::Rename => Box::new(RenameItem),
            FerriteAction::Delete => Box::new(RecycleItems),
            FerriteAction::CopyPath => return self.copy_paths(cx),
            FerriteAction::Properties => return self.start_shell_call(ShellCall::Properties),
            FerriteAction::ClassicMenu => return self.start_shell_call(ShellCall::Classic),
        };
        window.dispatch_action(list_action, cx);
    }

    fn copy_paths(&self, cx: &mut Context<Self>) {
        let Some(run) = &self.menu_run else {
            return;
        };
        let text: Vec<&str> = run.items.iter().map(Location::as_str).collect();
        let line_break = if cfg!(windows) { "\r\n" } else { "\n" };
        cx.write_to_clipboard(ClipboardItem::new_string(text.join(line_break)));
    }
}
