use gpui_kit::Div;
use gpui_kit::KeyBinding;
use rusty_core::Command;

use super::FerriteWindow;
use super::keys::PANE;
use crate::prelude::*;

gpui_kit::actions!(
    ferrite_tabs,
    [
        NextTab,
        PreviousTab,
        CloseTab,
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
    let pane = Some(PANE);
    cx.bind_keys([
        KeyBinding::new("ctrl-tab", NextTab, pane),
        KeyBinding::new("ctrl-pagedown", NextTab, pane),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, pane),
        KeyBinding::new("ctrl-pageup", PreviousTab, pane),
        KeyBinding::new("ctrl-w", CloseTab, pane),
        KeyBinding::new("ctrl-1", GoToTab1, pane),
        KeyBinding::new("ctrl-2", GoToTab2, pane),
        KeyBinding::new("ctrl-3", GoToTab3, pane),
        KeyBinding::new("ctrl-4", GoToTab4, pane),
        KeyBinding::new("ctrl-5", GoToTab5, pane),
        KeyBinding::new("ctrl-6", GoToTab6, pane),
        KeyBinding::new("ctrl-7", GoToTab7, pane),
        KeyBinding::new("ctrl-8", GoToTab8, pane),
        KeyBinding::new("ctrl-9", GoToLastTab, pane),
    ]);
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
    root.on_action(
        cx.listener(|this: &mut FerriteWindow, _: &CloseTab, window, cx| {
            this.apply(&Command::PaneCloseTab { tab: None }, window, cx);
        }),
    )
}
