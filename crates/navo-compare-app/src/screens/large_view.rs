//! The D5 Large view: three stage meters, four tiles, the slow-file note and a status tag.
//! The compare tab builds a `LargeSnapshot` after each drain and hands it to `set_snapshot`.

use gpui_kit::base::TestTextExt as _;
use gpui_kit::{AnimationExt, EventEmitter, FocusHandle, Focusable, KeyBinding, relative};
use sidewise_kit::prelude::*;
use sidewise_kit::tokens;
use sidewise_ui::browser::{ComparePhase, CompareTotals, SlowPath};

use super::large_model::{PhaseMeters, phase_label, reading_line, slow_note, tiles};
use crate::motion;

gpui_kit::actions!(large, [ShowFolders]);

/// Key context of the view's root element.
pub(crate) const CONTEXT: &str = "Large";

/// The stages in display order.
const PHASES: [ComparePhase; 3] = [
    ComparePhase::Listing,
    ComparePhase::QuickCheck,
    ComparePhase::ContentCheck,
];

/// What the Large view shows; the tab builds it after each drain.
#[derive(Debug, Clone, PartialEq)]
pub struct LargeSnapshot {
    pub meters: PhaseMeters,
    pub totals: CompareTotals,
    pub slow: Vec<SlowPath>,
    /// The oldest content read still running; `None` when none runs.
    pub reading: Option<SlowPath>,
    pub checking: bool,
}

/// What the view tells its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LargeEvent {
    ShowFolders,
}

/// The Large view: holds the newest snapshot and the meter fills it shows last.
pub struct LargeView {
    snapshot: LargeSnapshot,
    shown: [Option<f32>; 3],
    focus: FocusHandle,
}

impl EventEmitter<LargeEvent> for LargeView {}

impl Focusable for LargeView {
    /// The handle the tab focuses so `Enter` reaches the view.
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Binds the view's keys; the shell calls it from its own `bind_keys` once the tab is mounted.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("enter", ShowFolders, Some(CONTEXT))]);
}

impl LargeView {
    /// Makes an empty view in the "Comparing" state.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            snapshot: LargeSnapshot {
                meters: PhaseMeters::default(),
                totals: CompareTotals::default(),
                slow: Vec::new(),
                reading: None,
                checking: true,
            },
            shown: [Some(0.0); 3],
            focus: cx.focus_handle(),
        }
    }

    /// Replaces the snapshot, remembers the old fills so the new ones can animate from them.
    pub fn set_snapshot(&mut self, snapshot: LargeSnapshot, cx: &mut Context<Self>) {
        self.shown = PHASES.map(|phase| self.snapshot.meters.meter(phase).fraction());
        self.snapshot = snapshot;
        cx.notify();
    }

    /// The newest snapshot; for tests.
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> &LargeSnapshot {
        &self.snapshot
    }

    /// One stage: its heading, sub-line, counts and the animated bar.
    fn meter_view(&self, slot: usize, cx: &App) -> impl IntoElement {
        let t = tokens::current(cx);
        let phase = PHASES[slot];
        let (name, sub) = phase_label(phase);
        let meter = self.snapshot.meters.meter(phase);
        let count = match meter.total {
            Some(total) => format!("{} of {total}", meter.done),
            None => format!("{}", meter.done),
        };
        div()
            .flex()
            .flex_col()
            .gap(t.spacing.xs)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(t.typography.small)
                    .child(div().text_color(t.colors.text).child(name.test_text(name)))
                    .child(
                        div()
                            .text_color(t.colors.muted)
                            .font_features(t.typography.numbers_font_features.clone())
                            .child(count),
                    ),
            )
            .child(self.bar(slot, meter.fraction(), cx))
            .child(
                div()
                    .text_size(t.typography.tiny)
                    .text_color(t.colors.faint)
                    .child(sub),
            )
    }

    /// The track and fill; an unknown total shows a still stripe, never a loop.
    fn bar(&self, slot: usize, fraction: Option<f32>, cx: &App) -> impl IntoElement {
        let t = tokens::current(cx);
        let fill = div().h_full().rounded(t.radii.pill);
        let fill = match fraction {
            None => fill.w(relative(0.3)).bg(t.colors.faint).into_any_element(),
            Some(to) => {
                let from = self.shown[slot].unwrap_or(0.0);
                let fill = fill.bg(t.colors.accent);
                let id = format!("large-meter-{slot}-{to}");
                let ms = motion::row(cx).as_millis() as u64;
                match motion::anim(id.clone(), ms, cx).filter(|_| from != to) {
                    Some(enter) => fill
                        .with_animation(id, enter, move |el, delta| {
                            el.w(relative(from + (to - from) * delta))
                        })
                        .into_any_element(),
                    None => fill.w(relative(to)).into_any_element(),
                }
            }
        };
        div()
            .h(t.spacing.sm)
            .w_full()
            .overflow_hidden()
            .rounded(t.radii.pill)
            .bg(t.colors.line)
            .child(fill)
    }

    /// The row of four tiles with tabular numbers.
    fn tiles_view(&self, cx: &App) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .flex()
            .flex_wrap()
            .gap(t.spacing.md)
            .children(tiles(&self.snapshot.totals).map(|tile| {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(t.spacing.xxl * 6.)
                    .gap(t.spacing.xxs)
                    .p(t.spacing.md)
                    .rounded(t.radii.md)
                    .border_1()
                    .border_color(t.colors.line)
                    .bg(t.colors.surface)
                    .child(
                        div()
                            .text_size(t.typography.small)
                            .text_color(t.colors.muted)
                            .child(tile.label.test_text(tile.label)),
                    )
                    .child(
                        div()
                            .text_size(t.typography.body * 1.6)
                            .text_color(t.colors.text)
                            .font_features(t.typography.numbers_font_features.clone())
                            .child(tile.value.to_string()),
                    )
                    .child(
                        div()
                            .text_size(t.typography.tiny)
                            .text_color(t.colors.faint)
                            .child(tile.note),
                    )
            }))
    }

    /// The status tag: "Comparing" while the walk runs and "Finished" after.
    fn status_tag(&self, cx: &App) -> impl IntoElement {
        let t = tokens::current(cx);
        let (label, color) = if self.snapshot.checking {
            ("Comparing", t.colors.accent_text)
        } else {
            ("Finished", t.colors.ok)
        };
        div()
            .flex_none()
            .px(t.spacing.md)
            .py(t.spacing.xxs)
            .rounded(t.radii.pill)
            .border_1()
            .border_color(color)
            .text_size(t.typography.tiny)
            .text_color(color)
            .child(label.test_text(label))
    }
}

impl Render for LargeView {
    /// Draws the status tag, the meters, the running read, the tiles and the slow note inside the `Large` context.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx);
        let note = slow_note(&self.snapshot.slow);
        let reading = reading_line(self.snapshot.reading.as_ref());
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &ShowFolders, _, cx| cx.emit(LargeEvent::ShowFolders)))
            .flex()
            .flex_col()
            .size_full()
            .gap(t.spacing.lg)
            .p(t.spacing.xxl * 2.)
            .child(self.status_tag(cx))
            .children((0..PHASES.len()).map(|slot| self.meter_view(slot, cx)))
            .children(reading.map(|text| {
                div()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(text.clone().test_text(text))
            }))
            .child(self.tiles_view(cx))
            .children(note.map(|text| {
                div()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(text)
            }))
    }
}
