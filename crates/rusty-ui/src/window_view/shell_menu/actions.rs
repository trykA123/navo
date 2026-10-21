use gpui_kit::{Action, ClipboardItem};
use rusty_core::Location;
use rusty_core::shell_menu::FerriteAction;

use super::FerriteWindow;
use super::run::ShellCall;
use crate::file_list::{
    CopyItems, EditItem, MoveItems, NewFile, NewFolder, OpenRow, Paste, RecycleItems, Refresh,
    RenameItem, ViewItem,
};
use crate::prelude::*;

const UNKNOWN_ACTION: &str = "This menu item has no command";

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
            FerriteAction::NewFolder => Box::new(NewFolder),
            FerriteAction::NewFile => Box::new(NewFile),
            FerriteAction::Paste => Box::new(Paste),
            FerriteAction::Refresh => Box::new(Refresh),
            FerriteAction::SortBy => return,
            FerriteAction::CopyPath => return self.copy_paths(cx),
            FerriteAction::Properties => return self.start_shell_call(ShellCall::Properties),
            FerriteAction::ClassicMenu => return self.start_shell_call(ShellCall::Classic),
        };
        window.dispatch_action(list_action, cx);
    }

    pub(super) fn run_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        match cx.build_action(name, None) {
            Ok(action) => window.dispatch_action(action, cx),
            Err(error) => {
                debug_assert!(false, "menu item {name}: {error}");
                if let Some(run) = &self.menu_run {
                    let pane = run.pane;
                    self.report_pane_error(pane, format!("{UNKNOWN_ACTION} ({name})").into());
                }
            }
        }
    }

    fn copy_paths(&self, cx: &mut Context<Self>) {
        if let Some(run) = &self.menu_run {
            put_paths_on_clipboard(&run.items, cx);
        }
    }
}

pub(in crate::window_view) fn put_paths_on_clipboard(items: &[Location], cx: &mut App) {
    let text: Vec<&str> = items.iter().map(Location::as_str).collect();
    let line_break = if cfg!(windows) { "\r\n" } else { "\n" };
    cx.write_to_clipboard(ClipboardItem::new_string(text.join(line_break)));
}
