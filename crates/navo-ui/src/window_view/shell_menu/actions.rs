use gpui_kit::{Action, ClipboardItem};
use navo_core::Location;
use navo_core::shell_menu::NavoAction;

use super::NavoWindow;
use super::run::ShellCall;
use crate::file_list::{
    CopyItems, EditItem, MoveItems, NewFile, NewFolder, OpenRow, Paste, RecycleItems, Refresh,
    RenameItem, ViewItem,
};
use crate::prelude::*;

const UNKNOWN_ACTION: &str = "This menu item has no command";

impl NavoWindow {
    pub(super) fn run_navo(
        &mut self,
        action: NavoAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let list_action: Box<dyn Action> = match action {
            NavoAction::Open => Box::new(OpenRow),
            NavoAction::View => Box::new(ViewItem),
            NavoAction::Edit => Box::new(EditItem),
            NavoAction::Copy => Box::new(CopyItems),
            NavoAction::Move => Box::new(MoveItems),
            NavoAction::Rename => Box::new(RenameItem),
            NavoAction::Delete => Box::new(RecycleItems),
            NavoAction::NewFolder => Box::new(NewFolder),
            NavoAction::NewFile => Box::new(NewFile),
            NavoAction::Paste => Box::new(Paste),
            NavoAction::Refresh => Box::new(Refresh),
            NavoAction::SortBy => return,
            NavoAction::CopyPath => return self.copy_paths(cx),
            NavoAction::Properties => return self.start_shell_call(ShellCall::Properties),
            NavoAction::ClassicMenu => return self.start_shell_call(ShellCall::Classic),
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

    fn copy_paths(&mut self, _: &mut Context<Self>) {
        if let Some(run) = &self.menu_run {
            let items = run.target.locations().to_vec();
            self.lane.push(Box::new(move || {
                Box::new(move |_, _, cx| put_paths_on_clipboard(&items, cx))
            }));
        }
    }
}

pub(in crate::window_view) fn put_paths_on_clipboard(items: &[Location], cx: &mut App) {
    let text: Vec<&str> = items.iter().map(Location::as_str).collect();
    let line_break = if cfg!(windows) { "\r\n" } else { "\n" };
    cx.write_to_clipboard(ClipboardItem::new_string(text.join(line_break)));
}
