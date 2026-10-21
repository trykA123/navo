use std::rc::Rc;
use std::time::Instant;

use gpui_kit::{FocusHandle, ScrollHandle, Subscription, Task};

use super::FerriteWindow;
use crate::activity::{ActivityId, ActivityItem, ActivityKind};
use crate::activity_strip::{ChipsShown, Stop};
use crate::commands::active;
use crate::prelude::*;
use crate::primitives::Kbd;
use crate::tokens;

struct Timer {
    at: Instant,
    _task: Task<()>,
}

pub(super) struct StripUi {
    pub(super) line_focus: FocusHandle,
    pub(super) drawer_focus: FocusHandle,
    pub(super) is_drawer_open: bool,
    pub(super) selected: Option<Stop>,
    pub(super) items: Vec<ActivityItem>,
    pub(super) leaving: Option<Vec<ActivityItem>>,
    pub(super) chips_shown: ChipsShown,
    pub(super) drawer_scroll: ScrollHandle,
    is_hovered: bool,
    is_focused: bool,
    restore: Option<FocusHandle>,
    expiry: Option<Timer>,
    leave_timer: Option<Timer>,
    window: AnyWindowHandle,
    _focus: [Subscription; 4],
}

impl StripUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<FerriteWindow>) -> Self {
        let (line_focus, drawer_focus) = (cx.focus_handle(), cx.focus_handle());
        let focus = [
            cx.on_focus_in(&line_focus, window, FerriteWindow::strip_focus_changed),
            cx.on_focus_in(&drawer_focus, window, FerriteWindow::strip_focus_changed),
            cx.on_focus_out(&line_focus, window, |view, _, window, cx| {
                view.strip_focus_changed(window, cx);
            }),
            cx.on_focus_out(&drawer_focus, window, |view, _, window, cx| {
                view.strip_focus_changed(window, cx);
            }),
        ];
        Self {
            line_focus,
            drawer_focus,
            is_drawer_open: false,
            selected: None,
            items: Vec::new(),
            leaving: None,
            chips_shown: Rc::default(),
            drawer_scroll: ScrollHandle::new(),
            is_hovered: false,
            is_focused: false,
            restore: None,
            expiry: None,
            leave_timer: None,
            window: window.window_handle(),
            _focus: focus,
        }
    }

    pub(super) fn has_focus(&self, window: &Window) -> bool {
        self.line_focus.is_focused(window) || self.drawer_focus.is_focused(window)
    }
}

pub(super) fn undo_key(cx: &App) -> Option<SharedString> {
    let key = active(cx).keys_of("files.undo").next()?;
    Kbd::new("undo-key", key).map(|kbd| kbd.text().into())
}

fn wake_at(
    now: Instant,
    at: Instant,
    then: fn(&mut FerriteWindow, &mut Context<FerriteWindow>),
    cx: &mut Context<FerriteWindow>,
) -> Timer {
    let wait = at.saturating_duration_since(now);
    let task = cx.spawn(async move |this, cx| {
        cx.background_executor().timer(wait).await;
        let _ = this.update(cx, then);
    });
    Timer { at, _task: task }
}

impl FerriteWindow {
    pub(super) fn refresh_activity(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        self.job_ui.activity.set_undo_key(undo_key(cx));
        self.sync_activity(now);
        let activity = &self.job_ui.activity;
        let items: Vec<ActivityItem> = activity
            .primary()
            .into_iter()
            .chain(activity.rest())
            .cloned()
            .collect();
        let gone = std::mem::replace(&mut self.job_ui.strip.items, items);
        self.track_leaving(gone, now, cx);
        self.hold_done(now, cx);
        cx.notify();
    }

    fn track_leaving(&mut self, gone: Vec<ActivityItem>, now: Instant, cx: &mut Context<Self>) {
        let strip = &mut self.job_ui.strip;
        if !strip.items.is_empty() {
            strip.leaving = None;
            strip.leave_timer = None;
            return;
        }
        strip.is_hovered = false;
        strip.is_drawer_open = false;
        if !gone.is_empty() {
            let at = now + tokens::current(cx).motion.fast;
            strip.leaving = Some(gone);
            strip.leave_timer = Some(wake_at(now, at, Self::end_leaving, cx));
        }
        if strip.is_focused {
            self.leave_empty_strip(cx);
        }
    }

    fn end_leaving(&mut self, cx: &mut Context<Self>) {
        self.job_ui.strip.leaving = None;
        self.job_ui.strip.leave_timer = None;
        cx.notify();
    }

    fn leave_empty_strip(&self, cx: &mut Context<Self>) {
        let (handle, this) = (self.job_ui.strip.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |view, cx| view.leave_strip(window, cx));
            });
        });
    }

    fn hold_done(&mut self, now: Instant, cx: &mut Context<Self>) {
        let strip = &self.job_ui.strip;
        let held = strip.is_hovered || strip.is_focused;
        let done: Vec<ActivityId> = strip
            .items
            .iter()
            .filter(|item| item.kind == ActivityKind::Done)
            .map(|item| item.id)
            .collect();
        for id in done {
            self.job_ui.activity.hold(id, held, now);
        }
        let next = self.job_ui.activity.next_expiry();
        if self.job_ui.strip.expiry.as_ref().map(|timer| timer.at) != next {
            self.job_ui.strip.expiry = next.map(|at| wake_at(now, at, Self::refresh_activity, cx));
        }
    }

    pub(super) fn strip_hovered(&mut self, is_hovered: bool, cx: &mut Context<Self>) {
        self.job_ui.strip.is_hovered = is_hovered;
        self.hold_done(cx.background_executor().now(), cx);
        cx.notify();
    }

    pub(super) fn strip_focus_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_focused = self.job_ui.strip.has_focus(window);
        let strip = &mut self.job_ui.strip;
        strip.is_focused = is_focused;
        if !is_focused {
            strip.is_drawer_open = false;
            strip.selected = None;
            strip.restore = None;
        }
        self.hold_done(cx.background_executor().now(), cx);
        cx.notify();
    }

    pub(super) fn remember_focus(&mut self, window: &Window, cx: &App) {
        if !self.job_ui.strip.has_focus(window) {
            self.job_ui.strip.restore = window.focused(cx);
        }
    }

    pub(super) fn restore_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saved = self.job_ui.strip.restore.take();
        let viewer = self.viewer.as_ref().map(|viewer| viewer.focus_target(cx));
        match saved.or(viewer) {
            Some(handle) => window.focus(&handle, cx),
            None => self.focus_core_pane(window, cx),
        }
    }

    pub(super) fn has_activity_row(&self) -> bool {
        !self.job_ui.strip.items.is_empty() || self.job_ui.strip.leaving.is_some()
    }
}
