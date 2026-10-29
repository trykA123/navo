use gpui_kit::Div;
use rusty_core::{Command, TabCommand};

use super::FerriteWindow;
use super::shell_menu::put_paths_on_clipboard;
use crate::prelude::*;

gpui_kit::actions!(
    ferrite_tabs,
    [
        TogglePinTab,
        DuplicateTab,
        CloseOtherTabs,
        CloseTabsToRight,
        ReopenClosedTab,
        MoveTabToNewWorkspace,
        MoveTabToNewPane,
        CopyTabPath
    ]
);

impl FerriteWindow {
    fn run_tab(&mut self, command: TabCommand, window: &mut Window, cx: &mut Context<Self>) {
        self.apply(&Command::Tab(command), window, cx);
    }

    fn copy_tab_path(&mut self, cx: &mut Context<Self>) {
        let folder = self.browser.state().active_tab().base_or_location().clone();
        self.lane.push(Box::new(move || {
            Box::new(move |_, _, cx| put_paths_on_clipboard(&[folder], cx))
        }));
        cx.notify();
    }
}

macro_rules! run_tab {
    ($root:expr, $cx:expr, $($action:ident => $command:expr),+ $(,)?) => {
        $root$(.on_action($cx.listener(|this: &mut FerriteWindow, _: &$action, window, cx| {
            this.run_tab($command, window, cx);
        })))+
    };
}

pub(super) fn tab_actions(root: Div, cx: &mut Context<FerriteWindow>) -> Div {
    let root = run_tab!(root, cx,
        DuplicateTab => TabCommand::Duplicate { tab: None },
        CloseOtherTabs => TabCommand::CloseOthers { tab: None },
        CloseTabsToRight => TabCommand::CloseRight { tab: None },
        ReopenClosedTab => TabCommand::Reopen,
        MoveTabToNewWorkspace => TabCommand::MoveToNewWorkspace { tab: None },
        MoveTabToNewPane => TabCommand::MoveToNewPane { tab: None },
    );
    root.on_action(
        cx.listener(|this: &mut FerriteWindow, _: &TogglePinTab, window, cx| {
            let pinned = !this.browser.state().active_tab().is_pinned();
            this.run_tab(TabCommand::Pin { tab: None, pinned }, window, cx);
        }),
    )
    .on_action(
        cx.listener(|this: &mut FerriteWindow, _: &CopyTabPath, _, cx| {
            this.copy_tab_path(cx);
        }),
    )
}
