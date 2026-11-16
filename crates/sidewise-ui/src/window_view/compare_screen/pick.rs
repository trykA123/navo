//! Picked lines of the file diff (V32): Shift+↑/↓, Space, the tick boxes and Shift+click pick
//! lines on one side; Ctrl+Alt+←/→ then copy only those lines, Ctrl+Shift+Del removes picked
//! lines that exist only on one side, and Esc clears the pick.

mod state;

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::Stateful;

use crate::commands::compare::{PickDown, PickToggle, PickUp, RemovePicked};
use crate::compare_diff::{OnTick, PairText, PickSide};
use crate::prelude::*;
use crate::window_view::ScreenOwner;

pub(super) use state::{Pick, PickDraw, Spot};

#[cfg(test)]
mod tests;

impl ScreenOwner {
    /// Binds the pick keys on the file diff's `root`.
    pub(super) fn pick_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &PickDown, window, cx| {
            view.pick_with(window, cx, |pick, text, from| pick.step(text, true, from));
        }))
        .on_action(cx.listener(|view, _: &PickUp, window, cx| {
            view.pick_with(window, cx, |pick, text, from| pick.step(text, false, from));
        }))
        .on_action(cx.listener(|view, _: &PickToggle, window, cx| {
            view.pick_with(window, cx, Pick::toggle);
        }))
        .on_action(cx.listener(|view, _: &RemovePicked, _, cx| view.remove_picked(cx)))
    }

    /// Runs `change` on the picks of the open file diff and scrolls the line it lands on into
    /// view. Nothing happens in the Stream or before the text arrives.
    fn pick_with(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Pick, &PairText, Option<usize>) -> Option<Spot>,
    ) {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        let layout = file.look.layout_in(window);
        let Some(view) = file.view.as_mut() else {
            return;
        };
        let (text, from) = (Arc::clone(view.text()), view.current());
        if let Some(spot) = change(&mut file.pick, &text, from) {
            view.reveal(layout, spot);
            self.compare.diff.copy.note = None;
            cx.notify();
        }
    }

    /// A click on a tick box: picks the line, or the range from the last pick with Shift.
    fn pick_click(
        &mut self,
        (side, line, shift): (PickSide, u32, bool),
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.pick_with(window, cx, |pick, text, _| {
            pick.click(text, (side, line), shift)
        });
    }

    /// What the file diff's list draws for the picks, with a click that comes back here.
    pub(super) fn pick_draw(&self, cx: &mut Context<Self>) -> PickDraw {
        let weak = cx.weak_entity();
        let on_tick: OnTick = Rc::new(move |side, line, shift, window, cx| {
            let _ = weak.update(cx, |view, cx| {
                view.pick_click((side, line, shift), window, cx);
            });
        });
        let file = self.compare.screen.as_ref().and_then(|s| s.file.as_ref());
        match file {
            Some(file) => file.pick.draw(on_tick),
            None => Pick::default().draw(on_tick),
        }
    }

    /// Esc: drops the picks if there are any. Returns whether it did, so Esc goes no further.
    pub(super) fn clear_picks(&mut self) -> bool {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        file.is_some_and(|file| file.pick.clear())
    }

    /// Ctrl+Shift+Del: removes the picked lines that exist only on their side.
    fn remove_picked(&mut self, cx: &mut Context<Self>) {
        let file = self.compare.screen.as_ref().and_then(|s| s.file.as_ref());
        let resolved = file
            .and_then(|file| file.view.as_ref().map(|view| (file, view)))
            .ok_or("compare-copy-not-copyable")
            .and_then(|(file, view)| {
                let changes = file.pick.removal(view.text(), &file.request)?;
                Ok((changes, view.current()))
            });
        self.start_copy(resolved, true, cx);
    }
}
