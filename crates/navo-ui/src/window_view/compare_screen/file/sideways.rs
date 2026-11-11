//! The sideways keys of the compare screen (D39.3d, W8j): Left and Right move the open text
//! body — the file diff's view, else the large body — 8 columns. The shared state lives in
//! `compare_screen::sideways`; this file re-exports it to the `file` tree and wires the keys.

use gpui_kit::Stateful;

use crate::prelude::*;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::{ScrollLeft, ScrollRight};

pub(in crate::window_view::compare_screen::file) use crate::window_view::compare_screen::sideways::{
    SidewaysHandle, advance, on_wheel,
};
#[cfg(test)]
pub(in crate::window_view) use crate::window_view::compare_screen::sideways::SidewaysSeen;

/// How many columns Left or Right moves the text, as in the viewer (F3).
const KEY_COLS: f32 = 8.;

impl NavoWindow {
    /// Moves the open text body sideways by `cols` columns and redraws: the text view when it
    /// shows, else the large body the host draws — open, or held by a bytes switch (W8j2);
    /// with neither open the key goes on to the next handler.
    fn sideways_step(&mut self, cols: f32, cx: &mut Context<Self>) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let view = file
            .as_ref()
            .and_then(|file| file.view.as_ref().map(|view| view.sideways.clone()));
        let large = file.as_ref().and_then(|file| {
            file.large
                .as_ref()
                .or(file.switch.held_large())
                .map(|large| large.sideways())
        });
        let Some(handle) = view.or(large) else {
            cx.propagate();
            return;
        };
        if handle.step(cols) {
            cx.notify();
        }
    }

    /// Binds Left and Right (`viewer::ScrollLeft/ScrollRight`) on the file diff's `root`: they
    /// move the open text body 8 columns sideways (D39.3d).
    pub(in crate::window_view::compare_screen) fn sideways_keys(
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ScrollLeft, _, cx| {
            view.sideways_step(-KEY_COLS, cx);
        }))
        .on_action(cx.listener(|view, _: &ScrollRight, _, cx| {
            view.sideways_step(KEY_COLS, cx);
        }))
    }
}
