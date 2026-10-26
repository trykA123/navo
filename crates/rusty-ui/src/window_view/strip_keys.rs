use gpui_kit::{MouseButton, MouseDownEvent};

use super::FerriteWindow;
use super::keys::FocusActivity;
use crate::activity::{ActivityId, ActivityItem, ActivityKind};
use crate::activity_strip::{
    self as strip, Act, ActivityDrawer, ActivityStrip, DRAWER, STRIP, Stop, drawer_stops, settle,
    shown_line_stops,
};
use crate::commands::active;
use crate::prelude::*;
use crate::tokens;

fn keep_focus(_: &MouseDownEvent, window: &mut Window, _: &mut App) {
    window.prevent_default();
}

impl FerriteWindow {
    fn stops(&self) -> Vec<Stop> {
        let strip = &self.job_ui.strip;
        if strip.is_drawer_open {
            drawer_stops(&strip.items)
        } else {
            shown_line_stops(&strip.items, strip.chips_shown.get())
        }
    }

    fn remembers_focus(&self, host: Div, cx: &mut Context<Self>) -> Div {
        host.capture_any_mouse_down(cx.listener(|view, _: &MouseDownEvent, window, cx| {
            view.remember_focus(window, cx);
        }))
    }

    pub(super) fn activity_row(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let strip = &self.job_ui.strip;
        if let Some(items) = &strip.leaving {
            return Some(
                ActivityStrip::new(items.clone())
                    .leaving()
                    .into_any_element(),
            );
        }
        if strip.items.is_empty() {
            return None;
        }
        let is_focused = strip.line_focus.is_focused(window);
        let selected = settle(&self.stops(), strip.selected).filter(|_| is_focused);
        let key = active(cx)
            .keys_of("activity.focus")
            .next()
            .map(str::to_owned);
        let (on_stop, on_hover) = (cx.weak_entity(), cx.weak_entity());
        let line = ActivityStrip::new(strip.items.clone())
            .selected(selected)
            .focused(is_focused)
            .chips_shown(strip.chips_shown.clone())
            .all_key(key.as_deref())
            .on_stop(move |stop, window, cx| {
                let _ = on_stop.update(cx, |view, cx| view.click_stop(stop, window, cx));
            })
            .on_hover(move |is_hovered, _, cx| {
                let _ = on_hover.update(cx, |view, cx| view.strip_hovered(is_hovered, cx));
            });
        let host = div()
            .key_context(STRIP)
            .track_focus(&strip.line_focus)
            .flex_none()
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, keep_focus)
                    .child(line),
            );
        let host = self.remembers_focus(host, cx);
        Some(self.strip_keys(host, cx).into_any_element())
    }

    pub(super) fn activity_drawer_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let strip = &self.job_ui.strip;
        if !strip.is_drawer_open || strip.items.is_empty() {
            return None;
        }
        let t = tokens::current(cx).clone();
        let is_focused = strip.drawer_focus.is_focused(window);
        let selected = settle(&self.stops(), strip.selected).filter(|_| is_focused);
        let this = cx.weak_entity();
        let drawer = ActivityDrawer::new(strip.items.clone())
            .selected(selected)
            .focused(is_focused)
            .scroll(&strip.drawer_scroll)
            .on_stop(move |stop, window, cx| {
                let _ = this.update(cx, |view, cx| view.click_stop(stop, window, cx));
            });
        let inner = div()
            .key_context(DRAWER)
            .track_focus(&strip.drawer_focus)
            .on_action(cx.listener(|view, _: &strip::PreviousItem, _, cx| {
                view.move_stop(false, true, cx);
            }))
            .on_action(cx.listener(|view, _: &strip::NextItem, _, cx| {
                view.move_stop(true, true, cx);
            }))
            .child(drawer);
        let host = div()
            .key_context(STRIP)
            .absolute()
            .left(t.spacing.md)
            .right(t.spacing.md)
            .bottom(t.spacing.xs)
            .child(inner);
        let host = self.remembers_focus(host, cx);
        Some(self.strip_keys(host, cx).into_any_element())
    }

    fn strip_keys(&self, host: Div, cx: &mut Context<Self>) -> Div {
        host.on_action(cx.listener(|view, _: &strip::Previous, _, cx| {
            view.move_stop(false, false, cx);
        }))
        .on_action(cx.listener(|view, _: &strip::Next, _, cx| {
            view.move_stop(true, false, cx);
        }))
        .on_action(cx.listener(|view, _: &strip::Press, window, cx| {
            if let Some(stop) = settle(&view.stops(), view.job_ui.strip.selected) {
                view.run_stop(stop, window, cx);
            }
        }))
        .on_action(cx.listener(|view, _: &strip::PauseOrResume, _, cx| {
            view.pause_or_resume(cx);
        }))
        .on_action(cx.listener(|view, _: &strip::Remove, _, cx| {
            view.remove_activity(cx);
        }))
        .on_action(cx.listener(|view, _: &strip::Leave, window, cx| {
            view.leave_strip(window, cx);
        }))
    }

    pub(super) fn activity_keys(&self, root: Div, cx: &mut Context<Self>) -> Div {
        if self.job_ui.strip.items.is_empty() {
            return root;
        }
        root.on_action(cx.listener(|view, _: &FocusActivity, window, cx| {
            view.focus_activity(window, cx);
        }))
    }

    fn focus_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.job_ui.strip.items.len();
        if count == 0 {
            cx.propagate();
            return;
        }
        self.remember_focus(window, cx);
        self.job_ui.strip.is_drawer_open = count >= 2;
        self.job_ui.strip.selected = None;
        let handle = if self.job_ui.strip.is_drawer_open {
            self.job_ui.strip.drawer_focus.clone()
        } else {
            self.job_ui.strip.line_focus.clone()
        };
        window.focus(&handle, cx);
        self.strip_focus_changed(window, cx);
    }

    pub(super) fn leave_strip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.job_ui.strip.is_drawer_open = false;
        self.job_ui.strip.selected = None;
        if self.job_ui.strip.has_focus(window) {
            self.restore_focus(window, cx);
            self.strip_focus_changed(window, cx);
        }
        cx.notify();
    }

    fn move_stop(&mut self, forward: bool, by_item: bool, cx: &mut Context<Self>) {
        let stops = self.stops();
        let current = settle(&stops, self.job_ui.strip.selected);
        self.job_ui.strip.selected = if by_item {
            strip::step_item(&stops, current, forward)
        } else {
            strip::step(&stops, current, forward)
        };
        self.scroll_to_selected();
        cx.notify();
    }

    fn scroll_to_selected(&self) {
        let strip = &self.job_ui.strip;
        let item = strip.selected.and_then(Stop::item);
        let row = strip.items.iter().position(|each| Some(each.id) == item);
        if let Some(row) = row.filter(|_| strip.is_drawer_open) {
            strip.drawer_scroll.scroll_to_item(row + 1);
        }
    }

    fn target_item(&self) -> Option<ActivityItem> {
        let items = &self.job_ui.strip.items;
        let id = match settle(&self.stops(), self.job_ui.strip.selected) {
            Some(Stop::All) => return None,
            Some(stop) => stop.item(),
            None => items.first().map(|item| item.id),
        }?;
        items.iter().find(|item| item.id == id).cloned()
    }

    fn pause_or_resume(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.target_item().filter(|item| item.can_pause) else {
            return;
        };
        let act = if item.paused { Act::Resume } else { Act::Pause };
        self.run_act(item.id, act, cx);
    }

    fn remove_activity(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.target_item() else {
            return;
        };
        match item.kind {
            ActivityKind::Job if item.can_cancel => self.run_act(item.id, Act::Cancel, cx),
            ActivityKind::Background if item.can_cancel => self.run_act(item.id, Act::Stop, cx),
            ActivityKind::Failure | ActivityKind::Done => self.run_act(item.id, Act::Dismiss, cx),
            ActivityKind::Job | ActivityKind::Drag | ActivityKind::Background => {}
        }
    }

    fn click_stop(&mut self, stop: Stop, window: &mut Window, cx: &mut Context<Self>) {
        self.job_ui.strip.selected = Some(stop);
        self.run_stop(stop, window, cx);
        if !self.job_ui.strip.is_drawer_open {
            self.leave_strip(window, cx);
        }
    }

    fn run_stop(&mut self, stop: Stop, window: &mut Window, cx: &mut Context<Self>) {
        match stop {
            Stop::Button(_, Act::Show) => {
                self.leave_strip(window, cx);
                self.show_job_card(cx);
            }
            Stop::Button(id, act) => self.run_act(id, act, cx),
            Stop::Chip(id) => {
                self.job_ui.activity.pin(id);
                self.job_ui.strip.selected = None;
                self.refresh_activity(cx);
            }
            Stop::All => self.toggle_activity_drawer(window, cx),
            Stop::Row(_) => {}
        }
        cx.notify();
    }

    fn toggle_activity_drawer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.job_ui.strip.is_drawer_open {
            self.leave_strip(window, cx);
            return;
        }
        self.job_ui.strip.is_drawer_open = true;
        self.job_ui.strip.selected = None;
        window.focus(&self.job_ui.strip.drawer_focus.clone(), cx);
        self.strip_focus_changed(window, cx);
    }

    fn run_act(&mut self, id: ActivityId, act: Act, cx: &mut Context<Self>) {
        let ActivityId::Job(job) = id else {
            match act {
                Act::Dismiss => {
                    self.job_ui.activity.dismiss(id);
                    self.perf_dismissed(id);
                    self.refresh_activity(cx);
                }
                Act::Stop => self.stop_recording(cx),
                Act::OpenFolder => self.open_perf_folder(cx),
                Act::Retry => self.retry_perf_save(cx),
                Act::Pause | Act::Resume | Act::Cancel | Act::Show => {}
            }
            return;
        };
        match act {
            Act::Pause => self.pause_job(job, cx),
            Act::Resume => self.resume_job(job, cx),
            Act::Cancel | Act::Stop => self.cancel_job(job, cx),
            Act::Show => self.show_job_card(cx),
            Act::Dismiss => self.dismiss_job(job, cx),
            Act::OpenFolder | Act::Retry => {}
        }
    }
}
