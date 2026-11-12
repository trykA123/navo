//! Draws the outline drawer: folder headings, one row per file, and the rules count. Fixed at
//! the left in a wide window; over the cards below 900 px.

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use navo_l10n::{t, tr};

use super::{Line, lines};
use crate::compare_diff::card::badge;
use crate::compare_diff::{Layout, StreamItem};
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;
use crate::window_view::ScreenOwner;
use crate::window_view::compare_screen::stream::StreamScreen;

/// Width of the drawer, in px.
const WIDTH: f32 = 248.;
/// Height of a file or folder line, in px.
const ROW: f32 = 28.;
/// Size of the icons in a line, in px.
const ICON: f32 = 14.;
/// Indent of a file line under its folder, in px.
const INDENT: f32 = 22.;

/// A folder heading line.
fn folder_line(t: &Theme, dir: &str) -> Div {
    let name: SharedString = if dir.is_empty() {
        tr("compare-outline-top").into()
    } else {
        dir.to_owned().into()
    };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(ROW))
        .px(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(icon(IconName::Folder, px(ICON), t.colors.folder))
        .child(div().min_w_0().truncate().child(name))
}

/// The search-hit count pill.
fn hit_pill(t: &Theme, count: usize) -> Div {
    div()
        .flex_none()
        .px(t.spacing.sm)
        .rounded(t.radii.xs)
        .bg(t.colors.selection)
        .text_color(t.colors.on_selection)
        .font_family(t.typography.numbers_font.clone())
        .text_size(t.typography.small)
        .child(count.to_string())
}

/// The content of a file line: status mark, name, hit count, Viewed tick.
fn file_line(t: &Theme, stream: &StreamScreen, item: &StreamItem) -> Div {
    let viewed = stream.model.is_viewed(item.id);
    let name = item.path.rsplit_once('/').map_or(&*item.path, |(_, n)| n);
    let hits = stream.model.hit_count(item.id).filter(|&n| n > 0);
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(ROW))
        .pl(px(INDENT))
        .pr(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(if viewed {
            t.colors.muted
        } else {
            t.colors.text
        })
        .child(badge(t, item))
        .child(div().flex_1().min_w_0().truncate().child(name.to_owned()))
        .when_some(hits, |d, n| d.child(hit_pill(t, n)))
        .when(viewed, |d| {
            d.child(icon(IconName::Check, px(ICON), t.colors.muted))
        })
}

/// A clickable file line: the item in view has an accent bar, the picked one a wash.
fn file_row(
    t: &Theme,
    stream: &StreamScreen,
    (at, view): (usize, usize),
    cx: &mut Context<ScreenOwner>,
) -> AnyElement {
    let picked = stream.outline.selected == Some(at);
    let current = at == view;
    let click = cx.listener(move |this, _: &ClickEvent, window, cx| {
        this.outline_jump(at, window, cx);
    });
    div()
        .id(("compare-outline-row", at))
        .role(Role::ListItem)
        .aria_label(SharedString::from(stream.model.items()[at].path.clone()))
        .relative()
        .cursor_pointer()
        .when(picked, |d| d.bg(t.colors.hover))
        .hover(|d| d.bg(t.colors.hover))
        .when(current, |d| {
            d.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(t.colors.accent),
            )
        })
        .on_click(click)
        .child(file_line(t, stream, &stream.model.items()[at]))
        .into_any_element()
}

/// The rules line at the end.
fn rules_line(t: &Theme, count: usize) -> Div {
    div()
        .h(px(ROW))
        .px(t.spacing.md)
        .flex()
        .items_center()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(t!("compare-card-rules", count = count))
}

/// The drawer, or `None` while the outline is closed.
pub(in crate::window_view::compare_screen) fn drawer(
    t: &Theme,
    stream: &StreamScreen,
    cx: &mut Context<ScreenOwner>,
) -> Option<AnyElement> {
    if !stream.model.outline_open() {
        return None;
    }
    let view = stream.item_in_view();
    let rows: Vec<AnyElement> = lines(&stream.model)
        .into_iter()
        .map(|line| match line {
            Line::Folder(dir) => folder_line(t, &dir).into_any_element(),
            Line::File(at) => file_row(t, stream, (at, view), cx),
            Line::Rules(count) => rules_line(t, count).into_any_element(),
        })
        .collect();
    let narrow = stream.layout == Layout::Unified;
    let drawer = div()
        .id("compare-outline")
        .role(Role::List)
        .aria_label(tr("compare-outline-title"))
        .test_support()
        .key_context("CompareOutline")
        .track_focus(&stream.outline.focus)
        .flex_none()
        .flex()
        .flex_col()
        .w(px(WIDTH))
        .max_w(relative(0.86))
        .py(t.spacing.md)
        .border_r_1()
        .border_color(t.colors.line)
        .overflow_y_scroll()
        .track_scroll(&stream.outline.scroll);
    let drawer = if narrow {
        drawer
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .bg(t.colors.surface)
            .shadow(t.shadows.menu.clone())
            .occlude()
    } else {
        drawer.bg(t.colors.rail)
    };
    Some(drawer.children(rows).into_any_element())
}
