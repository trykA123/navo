//! Wires the viewer's View actions (drawers, presets, toggles) onto the window root.
//! Split from `view_menu.rs` so that file stays under the size limit.

use gpui_kit::{Action, Stateful};

use super::ViewOp;
use crate::prelude::*;
use crate::settings::Field;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::{
    Back, ChooseEncoding, ChooseLanguage, KeepViewDefault, ResetView, RightDrawer, ShowLineEnds,
    ToggleCharts, ToggleInfo, ToggleLineNumbers, ToggleMemoryStrip, ToggleOpensInspector,
    TogglePreview, ToggleRowBars, ToggleUsageBars, ToggleWhoUses, ViewCustom, ViewMenu, ViewRich,
    ViewSimple,
};

/// Binds action `A` on `root` to run the View operation `op`; returns the extended root.
fn on_view<A: Action>(
    root: Stateful<Div>,
    op: ViewOp,
    cx: &mut Context<NavoWindow>,
) -> Stateful<Div> {
    root.on_action(cx.listener(move |view, _: &A, window, cx| view.run_view(op, window, cx)))
}

impl NavoWindow {
    /// Binds every View menu action (drawers, toggles, presets) on the window `root`.
    pub(in crate::window_view::viewer) fn view_actions(
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let root = root
            .on_action(cx.listener(|view, _: &ViewMenu, window, cx| {
                view.open_view_menu(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ChooseEncoding, window, cx| {
                view.open_encoding_menu(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ChooseLanguage, window, cx| {
                view.open_language_menu(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ShowLineEnds, window, cx| {
                view.show_line_ends(window, cx);
            }))
            .on_action(cx.listener(|view, _: &TogglePreview, window, cx| {
                view.toggle_split(window, cx);
            }))
            .on_action(cx.listener(|view, _: &RightDrawer, window, cx| {
                view.right_drawer(window, cx);
            }))
            .on_action(cx.listener(|view, _: &Back, window, cx| view.close_drawer(window, cx)));
        let root = on_view::<ToggleLineNumbers>(root, ViewOp::Toggle(Field::TextLineNumbers), cx);
        let root = on_view::<ToggleInfo>(root, ViewOp::Toggle(Field::ImageDetails), cx);
        let root =
            on_view::<ToggleOpensInspector>(root, ViewOp::Toggle(Field::AutomotiveInspector), cx);
        let root =
            on_view::<ToggleMemoryStrip>(root, ViewOp::Toggle(Field::AutomotiveMemoryStrip), cx);
        let root = on_view::<ToggleCharts>(root, ViewOp::Toggle(Field::AutomotiveCharts), cx);
        let root = on_view::<ToggleRowBars>(root, ViewOp::Toggle(Field::AutomotiveRowBars), cx);
        let root = on_view::<ToggleUsageBars>(root, ViewOp::Toggle(Field::AutomotiveUsageBars), cx);
        let root = on_view::<ToggleWhoUses>(root, ViewOp::Toggle(Field::AutomotiveWhoUses), cx);
        let root = on_view::<ViewSimple>(root, ViewOp::Preset(0), cx);
        let root = on_view::<ViewRich>(root, ViewOp::Preset(1), cx);
        let root = on_view::<ViewCustom>(root, ViewOp::Preset(2), cx);
        let root = on_view::<KeepViewDefault>(root, ViewOp::Keep, cx);
        on_view::<ResetView>(root, ViewOp::Reset, cx)
    }
}
