use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FocusHandle, HighlightStyle, Role, StyledText};
use rusty_core::paging::Needle;

use super::count::{KEEP, count_search, needle_width};
use super::{Marked, Marking};
use crate::format;
use crate::prelude::*;
use crate::primitives::{ListScroll, RowSemantics, VirtualList};
use crate::tokens::{self, Theme};

pub(in crate::window_view::viewer) struct FindAll {
    needle: Needle,
    width: u64,
    starts: Option<Arc<Vec<u64>>>,
    pub selected: usize,
    scroll: ListScroll,
    pub focus: FocusHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct FoundRow {
    pub place: SharedString,
    pub text: SharedString,
    pub hit: Range<usize>,
}

const LEAD_CHARS: usize = 24;
const ELLIPSIS: &str = "…";

fn readable(text: &str) -> String {
    let mut line = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_control() {
            line.extend(std::iter::repeat_n(' ', ch.len_utf8()));
        } else {
            line.push(ch);
        }
    }
    line
}

fn floor_boundary(text: &str, at: usize) -> usize {
    (0..=at.min(text.len()))
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0)
}

pub(in crate::window_view::viewer) fn preview(
    place: String,
    line: &str,
    hit: Range<usize>,
) -> FoundRow {
    let line = readable(line);
    let start = floor_boundary(&line, hit.start);
    let end = (hit.end.min(line.len())..=line.len())
        .find(|at| line.is_char_boundary(*at))
        .unwrap_or(line.len())
        .max(start);
    let skip = line[..start].chars().count().saturating_sub(LEAD_CHARS);
    let cut = line.char_indices().nth(skip).map_or(0, |(at, _)| at);
    let lead = if cut > 0 { ELLIPSIS } else { "" };
    FoundRow {
        place: place.into(),
        text: format!("{lead}{}", &line[cut..]).into(),
        hit: start - cut + lead.len()..end - cut + lead.len(),
    }
}

pub(in crate::window_view::viewer) type RowSource = Rc<dyn Fn(u64, &mut App) -> Option<FoundRow>>;
pub(in crate::window_view::viewer) type OnMissing = Rc<dyn Fn(Vec<u64>, &mut App)>;

impl FindAll {
    fn selected_hit(&self) -> Option<Range<u64>> {
        let start = *self.starts.as_ref()?.get(self.selected)?;
        Some(start..start + self.width)
    }
}

impl Marking {
    pub(in crate::window_view::viewer) fn find_all_width(&self) -> Option<u64> {
        self.find_all.as_ref().map(|panel| panel.width)
    }

    pub(in crate::window_view::viewer) fn sync_find_all(&mut self) {
        let counted = self
            .count
            .as_ref()
            .filter(|run| run.counted.done)
            .map(|run| (&run.needle, &run.counted.starts));
        let Some(panel) = &mut self.find_all else {
            return;
        };
        if let Some((_, starts)) = counted.filter(|(needle, _)| **needle == panel.needle)
            && panel.starts.is_none()
        {
            panel.starts = Some(Arc::new(starts.clone()));
        }
    }
}

pub(in crate::window_view::viewer) fn open_find_all<V: Marked>(
    view: &mut V,
    needle: Needle,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    count_search(view, needle.clone(), cx);
    let focus = cx.focus_handle();
    window.focus(&focus, cx);
    view.marking().find_all = Some(FindAll {
        width: needle_width(&needle),
        needle,
        starts: None,
        selected: 0,
        scroll: ListScroll::new(),
        focus,
    });
    view.marking().sync_find_all();
    cx.notify();
}

pub(in crate::window_view::viewer) fn move_in_find_all<V: Marked>(
    view: &mut V,
    down: bool,
    cx: &mut Context<V>,
) {
    let Some(panel) = &mut view.marking().find_all else {
        return;
    };
    let len = panel.starts.as_ref().map_or(0, |starts| starts.len());
    let last = len.saturating_sub(1);
    panel.selected = if down {
        (panel.selected + 1).min(last)
    } else {
        panel.selected.saturating_sub(1)
    };
    panel.scroll.scroll_to(panel.selected);
    cx.notify();
}

pub(in crate::window_view::viewer) fn go_to_found<V: Marked>(
    view: &mut V,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let Some(hit) = view
        .marking()
        .find_all
        .as_ref()
        .and_then(FindAll::selected_hit)
    else {
        return;
    };
    view.jump(hit, window, cx);
    if let Some(panel) = &view.marking().find_all {
        window.focus(&panel.focus, cx);
    }
    cx.notify();
}

pub(in crate::window_view::viewer) fn close_find_all<V: Marked>(
    view: &mut V,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    view.marking().find_all = None;
    view.refocus(window, cx);
    cx.notify();
}

fn hit_style(t: &Theme) -> HighlightStyle {
    HighlightStyle {
        background_color: Some(t.colors.accent),
        color: Some(t.colors.background),
        ..HighlightStyle::default()
    }
}

fn found_row(t: &Theme, row: Option<FoundRow>, is_selected: bool) -> AnyElement {
    let Some(row) = row else {
        return div()
            .text_color(t.colors.faint)
            .child("…")
            .into_any_element();
    };
    let text = &row.text;
    let is_whole = row.hit.start < row.hit.end
        && row.hit.end <= text.len()
        && text.is_char_boundary(row.hit.start)
        && text.is_char_boundary(row.hit.end);
    let hit = is_whole.then(|| (row.hit.clone(), hit_style(t)));
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .size_full()
        .px(t.spacing.xl)
        .when(is_selected, |row| row.bg(t.colors.selection))
        .child(
            div()
                .flex()
                .flex_none()
                .justify_end()
                .w(t.spacing.size_column)
                .text_color(t.colors.muted)
                .child(row.place),
        )
        .child(
            div()
                .min_w_0()
                .whitespace_nowrap()
                .overflow_hidden()
                .child(StyledText::new(row.text).with_highlights(hit)),
        )
        .into_any_element()
}

pub(in crate::window_view::viewer) struct PanelRows {
    pub source: RowSource,
    pub on_missing: Option<OnMissing>,
}

fn list_rows(
    starts: Arc<Vec<u64>>,
    selected: usize,
    rows: PanelRows,
) -> impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static {
    move |range, _, cx| {
        let t = tokens::current(cx).clone();
        let wanted: Vec<u64> = starts[range.clone()].to_vec();
        let found: Vec<Option<FoundRow>> = wanted
            .iter()
            .map(|start| (rows.source)(*start, cx))
            .collect();
        let missing: Vec<u64> = wanted
            .iter()
            .zip(&found)
            .filter(|(_, row)| row.is_none())
            .map(|(start, _)| *start)
            .collect();
        if let Some(on_missing) = rows.on_missing.clone().filter(|_| !missing.is_empty()) {
            cx.defer(move |cx| on_missing(missing, cx));
        }
        found
            .into_iter()
            .zip(range)
            .map(|(row, index)| found_row(&t, row, index == selected))
            .collect()
    }
}

fn panel_title(marking: &Marking) -> String {
    let Some(counted) = marking.counted().filter(|counted| counted.done) else {
        return "Find all · counting…".to_owned();
    };
    let total = format::count(counted.total, "match", "matches");
    if counted.total > KEEP as u64 {
        format!(
            "Find all · {total} · the first {} listed",
            format::bytes(KEEP as u64)
        )
    } else {
        format!("Find all · {total}")
    }
}

pub(in crate::window_view::viewer) fn find_all_panel(
    t: &Theme,
    marking: &Marking,
    rows: PanelRows,
) -> Option<AnyElement> {
    let panel = marking.find_all.as_ref()?;
    let starts = panel.starts.clone().unwrap_or_default();
    let selected = panel.selected;
    let list = VirtualList::new(
        "find-all-list",
        starts.len(),
        &panel.scroll,
        list_rows(starts, selected, rows),
    )
    .label("Matches")
    .track_focus(&panel.focus)
    .describe_rows(move |index| RowSemantics {
        is_selected: Some(index == selected),
        is_active_descendant: index == selected,
        ..RowSemantics::default()
    });
    let title = panel_title(marking);
    Some(
        div()
            .id("find-all")
            .key_context(super::FIND_ALL_CONTEXT)
            .role(Role::Region)
            .aria_label(title.clone())
            .test_support()
            .flex()
            .flex_col()
            .flex_none()
            .h(t.spacing.drawer_height)
            .border_t_1()
            .border_color(t.colors.line)
            .bg(t.colors.surface)
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.small)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .h(t.spacing.row_height)
                    .px(t.spacing.xl)
                    .bg(t.colors.rail)
                    .font_family(t.typography.ui_font.clone())
                    .text_color(t.colors.muted)
                    .child(title),
            )
            .child(list)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::preview;

    #[test]
    fn a_short_line_keeps_its_hit_in_place() {
        let row = preview("7".to_owned(), "cache=miss here", 0..10);
        assert_eq!((row.text.as_ref(), row.hit), ("cache=miss here", 0..10));
    }

    #[test]
    fn a_far_hit_is_shown_after_an_ellipsis() {
        let line = format!("{}needle tail", "x".repeat(40));
        let row = preview(String::new(), &line, 40..46);
        assert!(row.text.starts_with('…'));
        assert_eq!(&row.text[row.hit.clone()], "needle");
    }

    #[test]
    fn control_characters_become_spaces_of_the_same_width() {
        let row = preview(String::new(), "a\tneedle", 2..8);
        assert_eq!(&row.text[row.hit.clone()], "needle");
        assert_eq!(row.text.as_ref(), "a needle");
    }
}
