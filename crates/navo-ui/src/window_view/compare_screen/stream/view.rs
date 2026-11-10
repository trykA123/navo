//! Draws the Stream: a header bar (back, title, sides, file count), the card list and one
//! status line with "Viewed n of N" over a thin bar, and binds the Stream's keys.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{ListState, Role, WeakEntity, list};
use navo_l10n::{t, tr};

use super::super::copy::{Note, note_view};
use super::super::{outline, search};
use super::StreamScreen;
use crate::compare_diff::{CardCtx, StreamRow, stream_row_view};
use crate::prelude::*;
use crate::primitives::{IconButton, IconName, icon};
use crate::tokens::{self, Theme};
use crate::window_view::NavoWindow;

/// The key context of the Stream (D27).
const CONTEXT: &str = "CompareStream";
/// Its parent context: Alt+↓/↑, Ctrl+Alt+←/→, Esc and the drawers are bound there once.
const PARENT: &str = "CompareView";
/// Width of the Viewed bar, in px.
const BAR_WIDTH: f32 = 64.;
/// Height of the Viewed bar, in px.
const BAR_HEIGHT: f32 = 4.;

/// Small muted text that may shrink and truncate.
fn small(t: &Theme) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

/// The header bar: back, the list icon, "Differences", the sides and the file count.
fn header(
    t: &Theme,
    stream: &StreamScreen,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let files = t!("compare-stream-files", count = stream.model.items().len());
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.md)
        .h(t.spacing.path_height)
        .border_b_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .child(
            IconButton::new(
                "compare-stream-back",
                IconName::ArrowLeft,
                tr("compare-screen-back"),
            )
            .on_click(back),
        )
        .child(icon(IconName::List, t.spacing.icon, t.colors.muted))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr("compare-stream-title")),
        )
        .child(small(t).child(stream.sides.clone()))
        .child(small(t).flex_none().child(files))
}

/// "Viewed n of N" with a thin bar filled by the viewed share.
fn counter(t: &Theme, (viewed, total): (usize, usize)) -> impl IntoElement {
    let text: SharedString = t!("compare-stream-viewed", viewed = viewed, total = total).into();
    let share = if total == 0 {
        0.
    } else {
        viewed as f32 / total as f32
    };
    let bar = div()
        .flex_none()
        .w(px(BAR_WIDTH))
        .h(px(BAR_HEIGHT))
        .rounded(px(BAR_HEIGHT / 2.))
        .overflow_hidden()
        .bg(t.surfaces.progress_track)
        .child(
            div()
                .h_full()
                .w(relative(share))
                .bg(t.surfaces.progress_fill),
        );
    div()
        .id("compare-stream-viewed")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.text)
        .child(bar)
        .child(text)
}

/// The status line: the Viewed counter, the last copy's note, then the keys and the way back.
fn status_line(t: &Theme, stream: &StreamScreen, note: Option<&Note>) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xl)
        .px(t.spacing.xl)
        .h(t.spacing.path_height)
        .border_t_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .child(counter(t, stream.model.counter()))
        .children(note.map(|note| note_view(t, note)))
        .child(div().flex_1())
        .child(small(t).flex_none().child(tr("compare-stream-keys")))
        .child(small(t).flex_none().child(tr("compare-screen-esc")))
}

/// One list row: the card row at `at`, inset, with a gap above each card but the first.
fn row(weak: &WeakEntity<NavoWindow>, at: usize, cx: &App) -> AnyElement {
    let Some(view) = weak.upgrade() else {
        return div().into_any_element();
    };
    let window = view.read(cx);
    let stream = window
        .compare
        .screen
        .as_ref()
        .and_then(|s| s.stream.as_ref());
    let Some(stream) = stream.filter(|stream| at < stream.rows.len()) else {
        return div().into_any_element();
    };
    let t = tokens::current(cx);
    let bars = window.card_bars(weak, cx);
    let ctx = CardCtx {
        t,
        model: &stream.model,
        layout: stream.layout,
        bars: Some(&bars),
    };
    let starts_card = matches!(
        stream.rows[at],
        StreamRow::Header(_) | StreamRow::RulesGroup
    );
    let menu = weak.clone();
    div()
        .px(t.spacing.lg)
        .when(starts_card, |d| d.pt(t.spacing.md))
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            let pointer = event.position;
            let _ = menu.update(cx, |view, cx| {
                view.menu_at_stream_row((at, pointer), window, cx);
            });
        })
        .child(stream_row_view(&stream.rows, at, &ctx))
        .into_any_element()
}

/// The card list over `state`, or the empty note when no file differs.
fn body(
    t: &Theme,
    state: &ListState,
    empty: Option<SharedString>,
    weak: WeakEntity<NavoWindow>,
) -> Div {
    let area = div()
        .id("compare-stream-rows")
        .role(Role::List)
        .aria_label(tr("compare-stream-rows"))
        .test_support()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .bg(t.colors.rail)
        .pb(t.spacing.md);
    let area = area.when_some(empty, |area, note| {
        area.child(
            div()
                .p(t.spacing.xxl)
                .text_color(t.colors.muted)
                .child(note),
        )
    });
    let rows = list(state.clone(), move |at, _, cx| row(&weak, at, cx)).size_full();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(area.child(rows))
}

impl NavoWindow {
    /// The Stream in place of the panes, or `None` when none is open.
    pub(in crate::window_view) fn stream_view(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let t = tokens::current(cx).clone();
        let back = cx.listener(|view, _: &ClickEvent, window, cx| view.leave_stream(window, cx));
        let weak = cx.weak_entity();
        let root = Self::stream_keys(div().id("compare-stream"), cx);
        let root = Self::search_keys(Self::outline_keys(root, cx), cx);
        let root = Self::copy_keys(Self::open_card_key(Self::sheet_keys(root, cx), cx), cx);
        let root = Self::stream_details_key(Self::look_actions(root, cx), cx);
        let root = Self::menu_keys(root, cx);
        let note = self.compare.diff.copy.note.clone();
        let stream = self.stream_mut()?;
        stream.relayout(stream.look.layout_in(window));
        let drawer = outline::drawer(&t, stream, cx);
        let details = super::details::drawer(&t, stream);
        let empty = stream.empty_note();
        let search = search::bar(&t, stream, cx);
        // The keys' context wraps the cards only: the search field sits outside it, so its
        // letters are typed and not run as J, K, V or O.
        let cards = root
            .key_context(CONTEXT)
            .track_focus(&stream.focus)
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .children(drawer)
            .child(body(&t, &stream.list, empty, weak))
            .children(details);
        let root = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(t.colors.surface)
            .child(header(&t, stream, back))
            .children(search)
            .child(cards)
            .child(status_line(&t, stream, note.as_ref()));
        let sheet = self.sheet_view(cx);
        let parent = div()
            .key_context(PARENT)
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(root)
            .children(sheet);
        Some(parent.into_any_element())
    }
}
