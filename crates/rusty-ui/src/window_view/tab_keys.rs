use gpui_kit::Div;
use rusty_core::Command;

use super::FerriteWindow;
use crate::prelude::*;

gpui_kit::actions!(
    ferrite_tabs,
    [
        NextTab,
        PreviousTab,
        CloseTab,
        CloneTab,
        GoToTab1,
        GoToTab2,
        GoToTab3,
        GoToTab4,
        GoToTab5,
        GoToTab6,
        GoToTab7,
        GoToTab8,
        GoToLastTab
    ]
);

pub(super) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "ferrite_tabs");
}

enum Target {
    Step(isize),
    Nth(usize),
    Last,
}

impl FerriteWindow {
    fn go_to_tab(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.browser.state();
        let Some(pane) = state.pane(state.focused_pane()) else {
            return;
        };
        let count = pane.tabs().len();
        if count == 0 {
            return;
        }
        let index = match target {
            Target::Step(step) => {
                let at = pane.active_index() as isize;
                (at + step).rem_euclid(count as isize) as usize
            }
            Target::Nth(n) if n < count => n,
            Target::Nth(_) => return,
            Target::Last => count - 1,
        };
        if let Ok(tab) = u32::try_from(index) {
            self.apply(&Command::PaneActivateTab { tab }, window, cx);
        }
    }
}

macro_rules! go_to {
    ($root:expr, $cx:expr, $($action:ident => $target:expr),+ $(,)?) => {
        $root$(.on_action($cx.listener(|this: &mut FerriteWindow, _: &$action, window, cx| {
            this.go_to_tab($target, window, cx);
        })))+
    };
}

pub(super) fn tab_keys(root: Div, cx: &mut Context<FerriteWindow>) -> Div {
    let root = go_to!(root, cx,
        NextTab => Target::Step(1),
        PreviousTab => Target::Step(-1),
        GoToTab1 => Target::Nth(0),
        GoToTab2 => Target::Nth(1),
        GoToTab3 => Target::Nth(2),
        GoToTab4 => Target::Nth(3),
        GoToTab5 => Target::Nth(4),
        GoToTab6 => Target::Nth(5),
        GoToTab7 => Target::Nth(6),
        GoToTab8 => Target::Nth(7),
        GoToLastTab => Target::Last,
    );
    let root = root.on_action(
        cx.listener(|this: &mut FerriteWindow, _: &CloneTab, window, cx| {
            let result = this.browser.clone_tab_to_other_pane();
            this.show_refusal(result);
            this.focus_core_pane(window, cx);
            cx.notify();
        }),
    );
    root.on_action(
        cx.listener(|this: &mut FerriteWindow, _: &CloseTab, window, cx| {
            this.apply(&Command::PaneCloseTab { tab: None }, window, cx);
        }),
    )
}
