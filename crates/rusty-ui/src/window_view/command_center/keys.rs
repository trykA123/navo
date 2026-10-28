use gpui_kit::Focusable;

use super::super::FerriteWindow;
use super::open::Way;
use crate::command_center::{
    Back, CopyPath, Down, First, Last, Mode, NextGroup, NextMode, OtherPane, PageDown, PageUp,
    PreviousGroup, PreviousMode, Results, Reveal, Row, Run, RunInNewTab, Selection, Toggle, Up,
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
        if open.step.is_none() && !open.mode.has_results() {
            return;
        }
        step(&mut open.selection, &open.rows);
        self.push_command_center(window, cx);
    }

    pub(in crate::window_view) fn command_center_on(
        &self,
        root: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        self.command_center_navigation(root, cx)
            .on_action(cx.listener(|view, _: &NextMode, window, cx| {
                view.cycle_mode(false, window, cx);
            }))
            .on_action(cx.listener(|view, _: &PreviousMode, window, cx| {
                view.cycle_mode(true, window, cx);
            }))
            .on_action(cx.listener(|view, _: &Run, window, cx| {
                view.run_mode(Way::Open, window, cx);
            }))
            .on_action(cx.listener(|view, _: &RunInNewTab, window, cx| {
                view.run_mode(Way::NewTab, window, cx);
            }))
            .on_action(cx.listener(|view, _: &Reveal, window, cx| {
                view.run_mode(Way::Reveal, window, cx);
            }))
            .on_action(cx.listener(|view, _: &OtherPane, window, cx| {
                view.run_mode(Way::OtherPane, window, cx);
            }))
            .on_action(cx.listener(|view, _: &CopyPath, window, cx| {
                view.run_mode(Way::CopyPath, window, cx);
            }))
            .on_action(cx.listener(|view, _: &Back, window, cx| {
                view.back_mode(window, cx);
            }))
    }

    fn command_center_navigation(&self, root: Div, cx: &mut Context<Self>) -> Div {
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
                view.move_selection(Selection::next_group, window, cx);
            }))
            .on_action(cx.listener(|view, _: &PreviousGroup, window, cx| {
                view.move_selection(Selection::previous_group, window, cx);
            }))
    }

    fn back_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        let center = open.center.clone();
        if open.step.is_some() || center.read(cx).query(cx).is_empty() {
            return self.command_center_back(window, cx);
        }
        center.update(cx, |center, cx| center.set_query("", window, cx));
        self.mode_query_changed("", window, cx);
    }

    fn run_mode(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .command_center
            .open
            .as_ref()
            .is_some_and(|open| open.step.is_some() || open.mode.has_results())
        {
            self.run_on_enter(way, window, cx);
        }
    }

    fn cycle_mode(&mut self, is_previous: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        if open.step.is_some() {
            if !is_previous {
                self.complete_path(window, cx);
            }
            return;
        }
        let mode = open.center.read(cx).mode(cx).cycle(is_previous);
        self.change_mode(mode, window, cx);
    }

    pub(super) fn change_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        if open.step.is_some() {
            return;
        }
        let query = open.center.read(cx).query(cx);
        let query = query.strip_prefix('>').unwrap_or(&query).to_owned();
        open.mode = mode;
        open.rows = Default::default();
        open.rows_memory.set(Results::memory_use(&open.rows));
        open.selection = Selection::start(&open.rows);
        open.center
            .update(cx, |center, cx| center.set_query(&query, window, cx));
        window.focus(&open.center.focus_handle(cx), cx);
        self.mode_query_changed(&query, window, cx);
    }

    pub(super) fn mode_query_changed(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        if open.step.is_some() {
            return self.query_changed(query, window, cx);
        }
        if query.starts_with('>') {
            open.mode = Mode::Commands;
        }
        if !open.mode.has_results() {
            open.search.cancel();
            open.run_when_shown = None;
            self.push_command_center(window, cx);
            return;
        }
        let query = if open.mode == Mode::Commands {
            format!(">{}", query.trim_start_matches(['>', '/']))
        } else {
            query.to_owned()
        };
        self.push_command_center(window, cx);
        self.query_changed(&query, window, cx);
    }
}
