use std::collections::HashSet;
use std::hash::Hash;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{FocusHandle, Focusable, Subscription};
use rusty_l10n::t;

use super::bar::{BarLook, bar_row};
use crate::format;
use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::tokens::Theme;

pub(in crate::window_view::viewer) struct Found<H> {
    hits: Vec<H>,
    set: HashSet<H>,
    current: usize,
}

impl<H: Copy + Eq + Hash> Found<H> {
    pub(in crate::window_view::viewer) fn new(hits: Vec<H>, start: usize) -> Self {
        let current = start.min(hits.len().saturating_sub(1));
        let set = hits.iter().copied().collect();
        Self { hits, set, current }
    }

    fn current(&self) -> Option<H> {
        self.hits.get(self.current).copied()
    }

    fn step(&mut self, backward: bool) -> Option<H> {
        let len = self.hits.len();
        if len == 0 {
            return None;
        }
        self.current = if backward {
            (self.current + len - 1) % len
        } else {
            (self.current + 1) % len
        };
        self.current()
    }

    fn label(&self) -> String {
        if self.hits.is_empty() {
            return t!("viewer-find-none");
        }
        t!(
            "viewer-find-place",
            place = format::bytes(self.current as u64 + 1),
            total = format::bytes(self.hits.len() as u64)
        )
    }
}

pub(in crate::window_view::viewer) struct ViewFind<H> {
    pub input: Entity<TextFieldState>,
    found: Option<Found<H>>,
    _events: Subscription,
}

impl<H: Copy + Eq + Hash> ViewFind<H> {
    pub(in crate::window_view::viewer) fn is_current(&self, hit: H) -> bool {
        self.found.as_ref().and_then(Found::current) == Some(hit)
    }

    pub(in crate::window_view::viewer) fn is_hit(&self, hit: H) -> bool {
        self.found
            .as_ref()
            .is_some_and(|found| found.set.contains(&hit))
    }
}

pub(in crate::window_view::viewer) trait Findable:
    Sized + 'static
{
    type Hit: Copy + Eq + Hash + 'static;

    fn find_slot(&mut self) -> &mut Option<ViewFind<Self::Hit>>;
    fn hits_for(&self, query: &str) -> (Vec<Self::Hit>, usize);
    fn show_hit(&mut self, hit: Self::Hit, cx: &mut Context<Self>);
    fn body_focus(&self) -> FocusHandle;
}

pub(in crate::window_view::viewer) fn open_find<V: Findable>(
    view: &mut V,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if let Some(find) = view.find_slot() {
        let focus = find.input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        return;
    }
    let input = text_field_state("Find", window, cx);
    let events = cx.subscribe_in(
        &input,
        window,
        |view: &mut V, _, event: &InputEvent, _, cx| match event {
            InputEvent::PressEnter { shift, .. } => step_find(view, *shift, cx),
            InputEvent::Change => retarget_find(view, cx),
            _ => {}
        },
    );
    let focus = input.read(cx).focus_handle(cx);
    window.focus(&focus, cx);
    *view.find_slot() = Some(ViewFind {
        input,
        found: None,
        _events: events,
    });
    cx.notify();
}

pub(in crate::window_view::viewer) fn close_find<V: Findable>(
    view: &mut V,
    window: &mut Window,
    cx: &mut Context<V>,
) -> bool {
    if view.find_slot().take().is_none() {
        return false;
    }
    window.focus(&view.body_focus(), cx);
    cx.notify();
    true
}

pub(in crate::window_view::viewer) fn find_again<V: Findable>(
    view: &mut V,
    backward: bool,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if view.find_slot().is_some() {
        step_find(view, backward, cx);
    } else {
        open_find(view, window, cx);
    }
}

pub(in crate::window_view::viewer) fn is_find_focused<H>(
    find: Option<&ViewFind<H>>,
    window: &Window,
    cx: &App,
) -> bool {
    find.is_some_and(|find| {
        find.input
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx)
    })
}

fn retarget_find<V: Findable>(view: &mut V, cx: &mut Context<V>) {
    let Some(find) = view.find_slot() else {
        return;
    };
    let query = find.input.read(cx).value().to_string();
    let found = (!query.is_empty()).then(|| {
        let (hits, start) = view.hits_for(&query);
        Found::new(hits, start)
    });
    let current = found.as_ref().and_then(Found::current);
    if let Some(find) = view.find_slot() {
        find.found = found;
    }
    if let Some(hit) = current {
        view.show_hit(hit, cx);
    }
    cx.notify();
}

fn step_find<V: Findable>(view: &mut V, backward: bool, cx: &mut Context<V>) {
    let hit = view
        .find_slot()
        .as_mut()
        .and_then(|find| find.found.as_mut())
        .and_then(|found| found.step(backward));
    if let Some(hit) = hit {
        view.show_hit(hit, cx);
    }
    cx.notify();
}

pub(in crate::window_view::viewer) fn find_strip<H: Copy + Eq + Hash>(
    t: &Theme,
    find: &ViewFind<H>,
) -> impl IntoElement {
    let look = BarLook {
        id: "viewer-find",
        input_id: "viewer-find-input",
        error_id: "viewer-find-error",
        label: "Find",
        input: &find.input,
        is_find: true,
        match_case: None,
        count: find.found.as_ref().map(|found| found.label().into()),
        error: None,
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .px(t.spacing.xl)
        .h(t.spacing.path_height)
        .border_b_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .child(bar_row(t, look, |_, _, _| {}))
}

pub(in crate::window_view::viewer) fn contains_ignore_case(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.is_ascii() {
        let needle = needle.as_bytes();
        return text
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle));
    }
    text.to_lowercase().contains(&needle.to_lowercase())
}

pub(in crate::window_view::viewer) fn hit_tint(t: &Theme, is_current: bool) -> Hsla {
    if is_current {
        t.colors.accent.opacity(0.35)
    } else {
        t.marks.ticks[1].opacity(0.35)
    }
}
