use std::time::Instant;

use gpui_kit::{FocusHandle, Task};

use super::FerriteWindow;
use super::keys::FocusActivity;
use crate::activity::{ActivityId, ActivityItem, ActivityKind};
use crate::activity_strip::{
    self as strip, Act, ActivityDrawer, ActivityStrip, DRAWER, STRIP, Stop, drawer_stops,
    line_stops, settle,
};
use crate::commands::active;
use crate::prelude::*;
use crate::tokens;

struct Timer {
    at: Instant,
    _task: Task<()>,
}

pub(super) struct StripUi {
    line_focus: FocusHandle,
    drawer_focus: FocusHandle,
    is_drawer_open: bool,
    selected: Option<Stop>,
    is_hovered: bool,
    expiry: Option<Timer>,
    shown: Vec<ActivityItem>,
    leaving: Option<(Vec<ActivityItem>, Timer)>,
}

impl StripUi {
    pub(super) fn new(cx: &App) -> Self {
        Self {
            line_focus: cx.focus_handle(),
            drawer_focus: cx.focus_handle(),
            is_drawer_open: false,
            selected: None,
            is_hovered: false,
            expiry: None,
            shown: Vec::new(),
            leaving: None,
        }
    }

    fn is_focused(&self, window: &Window) -> bool {
        self.line_focus.is_focused(window) || self.drawer_focus.is_focused(window)
    }
}

fn wake_at(now: Instant, at: Instant, cx: &mut Context<FerriteWindow>) -> Timer {
    let wait = at.saturating_duration_since(now);
    let task = cx.spawn(async move |this, cx| {
        cx.background_executor().timer(wait).await;
        let _ = this.update(cx, |_, cx| cx.notify());
    });
    Timer { at, _task: task }
}

impl FerriteWindow {
    fn activity_items(&self) -> Vec<ActivityItem> {
        let activity = &self.job_ui.activity;
        activity
            .primary()
            .into_iter()
            .chain(activity.rest())
            .cloned()
            .collect()
    }

    pub(super) fn refresh_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        self.sync_activity(now);
        let held = self.job_ui.strip.is_hovered || self.job_ui.strip.is_focused(window);
        let done: Vec<ActivityId> = self
            .activity_items()
            .iter()
            .filter(|item| item.kind == ActivityKind::Done)
            .map(|item| item.id)
            .collect();
        for id in done {
            self.job_ui.activity.hold(id, held, now);
        }
        let next = self.job_ui.activity.next_expiry();
        if self.job_ui.strip.expiry.as_ref().map(|timer| timer.at) != next {
            self.job_ui.strip.expiry = next.map(|at| wake_at(now, at, cx));
        }
        self.track_leaving(now, cx);
        self.settle_strip_focus(window, cx);
    }

    fn track_leaving(&mut self, now: Instant, cx: &mut Context<Self>) {
        let items = self.activity_items();
        if !items.is_empty() {
            self.job_ui.strip.leaving = None;
            self.job_ui.strip.shown = items;
            return;
        }
        if self
            .job_ui
            .strip
            .leaving
            .as_ref()
            .is_some_and(|(_, timer)| timer.at <= now)
        {
            self.job_ui.strip.leaving = None;
        }
        self.job_ui.strip.is_hovered = false;
        let shown = std::mem::take(&mut self.job_ui.strip.shown);
        if !shown.is_empty() {
            let at = now + tokens::current(cx).motion.fast;
            self.job_ui.strip.leaving = Some((shown, wake_at(now, at, cx)));
        }
    }

    fn settle_strip_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_empty = self.job_ui.activity.is_empty();
        let is_focused = self.job_ui.strip.is_focused(window);
        if is_empty || !is_focused {
            self.job_ui.strip.is_drawer_open = false;
        }
        if is_empty && is_focused {
            self.focus_core_pane(window, cx);
        }
    }

    pub(super) fn has_activity_row(&self) -> bool {
        !self.job_ui.activity.is_empty() || self.job_ui.strip.leaving.is_some()
    }

    fn stops_of(&self, items: &[ActivityItem]) -> Vec<Stop> {
        if self.job_ui.strip.is_drawer_open {
            drawer_stops(items)
        } else {
            line_stops(items)
        }
    }

    pub(super) fn activity_row(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some((items, _)) = &self.job_ui.strip.leaving {
            return Some(
                ActivityStrip::new(items.clone())
                    .leaving()
                    .into_any_element(),
            );
        }
        let items = self.activity_items();
        if items.is_empty() {
            return None;
        }
        let is_focused = self.job_ui.strip.line_focus.is_focused(window);
        let selected =
            settle(&line_stops(&items), self.job_ui.strip.selected).filter(|_| is_focused);
        let key = active(cx)
            .keys_of("activity.focus")
            .next()
            .map(str::to_owned);
        let (on_stop, on_hover) = (cx.weak_entity(), cx.weak_entity());
        let line = ActivityStrip::new(items)
            .selected(selected)
            .all_key(key.as_deref())
            .on_stop(move |stop, window, cx| {
                let _ = on_stop.update(cx, |view, cx| view.click_stop(stop, window, cx));
            })
            .on_hover(move |is_hovered, _, cx| {
                let _ = on_hover.update(cx, |view, cx| {
                    view.job_ui.strip.is_hovered = is_hovered;
                    cx.notify();
                });
            });
        let host = div()
            .key_context(STRIP)
            .track_focus(&self.job_ui.strip.line_focus)
            .flex_none()
            .child(line);
        Some(self.strip_keys(host, cx).into_any_element())
    }

    pub(super) fn activity_drawer_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let items = self.activity_items();
        if !self.job_ui.strip.is_drawer_open || items.is_empty() {
            return None;
        }
        let t = tokens::current(cx).clone();
        let is_focused = self.job_ui.strip.drawer_focus.is_focused(window);
        let selected =
            settle(&drawer_stops(&items), self.job_ui.strip.selected).filter(|_| is_focused);
        let this = cx.weak_entity();
        let drawer =
            ActivityDrawer::new(items)
                .selected(selected)
                .on_stop(move |stop, window, cx| {
                    let _ = this.update(cx, |view, cx| view.click_stop(stop, window, cx));
                });
        let inner = div()
            .key_context(DRAWER)
            .track_focus(&self.job_ui.strip.drawer_focus)
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
            let items = view.activity_items();
            if let Some(stop) = settle(&view.stops_of(&items), view.job_ui.strip.selected) {
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
        if self.job_ui.activity.is_empty() {
            return root;
        }
        root.on_action(cx.listener(|view, _: &FocusActivity, window, cx| {
            view.focus_activity(window, cx);
        }))
    }

    fn focus_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.job_ui.activity.len();
        if count == 0 {
            cx.propagate();
            return;
        }
        self.job_ui.strip.is_drawer_open = count >= 2;
        self.job_ui.strip.selected = None;
        let handle = if self.job_ui.strip.is_drawer_open {
            self.job_ui.strip.drawer_focus.clone()
        } else {
            self.job_ui.strip.line_focus.clone()
        };
        window.focus(&handle, cx);
        cx.notify();
    }

    fn leave_strip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.job_ui.strip.is_drawer_open = false;
        self.job_ui.strip.selected = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    fn move_stop(&mut self, forward: bool, by_item: bool, cx: &mut Context<Self>) {
        let items = self.activity_items();
        let stops = self.stops_of(&items);
        let current = settle(&stops, self.job_ui.strip.selected);
        self.job_ui.strip.selected = if by_item {
            strip::step_item(&stops, current, forward)
        } else {
            strip::step(&stops, current, forward)
        };
        cx.notify();
    }

    fn target_item(&self) -> Option<ActivityItem> {
        let items = self.activity_items();
        let target =
            settle(&self.stops_of(&items), self.job_ui.strip.selected).and_then(Stop::item);
        let primary = items.first().map(|item| item.id);
        let id = target.or(primary)?;
        items.into_iter().find(|item| item.id == id)
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
            ActivityKind::Failure | ActivityKind::Done => self.run_act(item.id, Act::Dismiss, cx),
            ActivityKind::Job | ActivityKind::Drag | ActivityKind::Background => {}
        }
    }

    fn click_stop(&mut self, stop: Stop, window: &mut Window, cx: &mut Context<Self>) {
        self.job_ui.strip.selected = Some(stop);
        self.run_stop(stop, window, cx);
        if !self.job_ui.strip.is_drawer_open {
            self.job_ui.strip.selected = None;
            self.focus_core_pane(window, cx);
        }
    }

    fn run_stop(&mut self, stop: Stop, window: &mut Window, cx: &mut Context<Self>) {
        match stop {
            Stop::Button(id, act) => self.run_act(id, act, cx),
            Stop::Chip(id) => {
                self.job_ui.activity.pin(id);
                self.job_ui.strip.selected = None;
            }
            Stop::All => self.toggle_activity_drawer(window, cx),
            Stop::Row(_) => {}
        }
        cx.notify();
    }

    fn toggle_activity_drawer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.job_ui.strip.is_drawer_open = !self.job_ui.strip.is_drawer_open;
        self.job_ui.strip.selected = None;
        if self.job_ui.strip.is_drawer_open {
            window.focus(&self.job_ui.strip.drawer_focus.clone(), cx);
        } else {
            self.focus_core_pane(window, cx);
        }
    }

    fn run_act(&mut self, id: ActivityId, act: Act, cx: &mut Context<Self>) {
        let ActivityId::Job(job) = id else {
            if act == Act::Dismiss {
                self.job_ui.activity.dismiss(id);
            }
            return;
        };
        match act {
            Act::Pause => self.pause_job(job, cx),
            Act::Resume => self.resume_job(job, cx),
            Act::Cancel => self.cancel_job(job, cx),
            Act::Show => self.show_job_card(cx),
            Act::Dismiss => self.dismiss_job(job, cx),
        }
    }
}
