use super::super::FerriteWindow;
use crate::command_center::{
    Back, Down, First, Last, NextGroup, PageDown, PageUp, PreviousGroup, Row, Run, RunInNewTab,
    Selection, Toggle, Up,
};
use crate::prelude::*;

const PAGE: isize = 12;

impl FerriteWindow {
    fn move_selection(
        &mut self,
        step: impl FnOnce(&mut Selection, &[Row]),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        step(&mut open.selection, &open.rows);
        self.push_command_center(window, cx);
    }

    pub(in crate::window_view) fn command_center_on(
        &self,
        root: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        root.children(self.command_center_view())
            .on_action(cx.listener(|view, _: &Toggle, window, cx| {
                view.toggle_command_center(window, cx);
            }))
            .on_action(cx.listener(|view, _: &Up, window, cx| {
                view.move_selection(|at, rows| at.step(rows, -1), window, cx);
            }))
            .on_action(cx.listener(|view, _: &Down, window, cx| {
                view.move_selection(|at, rows| at.step(rows, 1), window, cx);
            }))
            .on_action(cx.listener(|view, _: &PageUp, window, cx| {
                view.move_selection(|at, rows| at.step(rows, -PAGE), window, cx);
            }))
            .on_action(cx.listener(|view, _: &PageDown, window, cx| {
                view.move_selection(|at, rows| at.step(rows, PAGE), window, cx);
            }))
            .on_action(cx.listener(|view, _: &First, window, cx| {
                view.move_selection(Selection::first, window, cx);
            }))
            .on_action(cx.listener(|view, _: &Last, window, cx| {
                view.move_selection(Selection::last, window, cx);
            }))
            .on_action(cx.listener(|view, _: &NextGroup, window, cx| {
                if !view.complete_path(window, cx) {
                    view.move_selection(Selection::next_group, window, cx);
                }
            }))
            .on_action(cx.listener(|view, _: &PreviousGroup, window, cx| {
                view.move_selection(Selection::previous_group, window, cx);
            }))
            .on_action(cx.listener(|view, _: &Run, window, cx| {
                view.run_on_enter(false, window, cx);
            }))
            .on_action(cx.listener(|view, _: &RunInNewTab, window, cx| {
                view.run_on_enter(true, window, cx);
            }))
            .on_action(cx.listener(|view, _: &Back, window, cx| {
                view.command_center_back(window, cx);
            }))
    }
}
