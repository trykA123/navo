use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Div, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Pixels, Role, StatefulInteractiveElement, Styled, div, px,
};
use navo_fs::{EntryKind, FsEntry};
use navo_l10n::tr;

use super::row::check_column;
use super::{FileListAction, ListContext, git_text};
use crate::git::{BadgeKind, FolderBadges, RepoBadge};
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::Theme;

const STALE_OPACITY: f32 = 0.5;
const DOT: Pixels = px(4.);

/// What a row needs to draw its gutter cell. A `None` badge leaves the cell empty.
#[derive(Clone, Copy)]
pub(super) struct GitSlot<'a> {
    pub badge: Option<&'a RepoBadge>,
    pub ctx: &'a Rc<ListContext>,
    pub row: u64,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Mark {
    Glyph(IconName, Hsla),
    Dot(Hsla),
}

/// The gutter shows only when the listing holds a repository.
pub(super) fn present(git: Option<&Arc<FolderBadges>>, find: Option<bool>) -> bool {
    find.is_none() && git.is_some_and(|git| !git.by_name.is_empty())
}

pub(super) fn width(t: &Theme) -> Pixels {
    check_column(t)
}

pub(super) fn slot<'a>(
    ctx: &'a Rc<ListContext>,
    (entry, row): (&FsEntry, u64),
) -> Option<GitSlot<'a>> {
    let git = ctx.props.git.as_ref();
    if !present(git, ctx.find_results) {
        return None;
    }
    let badge = git
        .filter(|_| entry.kind == EntryKind::Folder)
        .and_then(|git| git.by_name.get(&entry.name.to_text()));
    Some(GitSlot { badge, ctx, row })
}

pub(crate) fn mark(t: &Theme, badge: &RepoBadge) -> Mark {
    let g = &t.git;
    match badge.kind {
        BadgeKind::Conflict => Mark::Glyph(IconName::TriangleAlert, g.conflict),
        BadgeKind::Modified => Mark::Glyph(IconName::CircleDot, g.modified),
        BadgeKind::Ahead => Mark::Glyph(IconName::ArrowUp, g.sync),
        BadgeKind::Behind => Mark::Glyph(IconName::ArrowDown, g.sync),
        BadgeKind::Diverged => Mark::Glyph(IconName::ArrowUpDown, g.sync),
        BadgeKind::Clean if badge.is_root => Mark::Glyph(IconName::CircleCheck, t.colors.ok),
        BadgeKind::Clean => Mark::Dot(g.quiet),
        BadgeKind::Unknown => Mark::Glyph(IconName::CircleQuestionMark, t.colors.muted),
        BadgeKind::Checking => Mark::Glyph(IconName::CircleDashed, g.quiet),
    }
}

fn mark_element(t: &Theme, mark: Mark) -> AnyElement {
    match mark {
        Mark::Glyph(name, color) => icon(name, t.spacing.icon - px(1.), color).into_any_element(),
        Mark::Dot(color) => div()
            .size(DOT)
            .rounded(t.radii.full)
            .bg(color)
            .into_any_element(),
    }
}

fn frame(t: &Theme) -> Div {
    div()
        .w(width(t))
        .flex_none()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
}

fn button(t: &Theme, badge: &RepoBadge, slot: &GitSlot) -> AnyElement {
    let (ctx, row) = (slot.ctx.clone(), slot.row);
    let tip = git_text::tooltip(badge);
    frame(t)
        .id("git-badge")
        .role(Role::Button)
        .aria_label(git_text::spoken(badge))
        .test_support()
        .rounded(t.radii.sm)
        .cursor_pointer()
        .when(badge.stale, |d| d.opacity(STALE_OPACITY))
        .hover(|d| d.bg(t.colors.hover))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            let at = event.position();
            ctx.act(FileListAction::GitMenu { row, at }, window, cx);
        })
        .child(mark_element(t, mark(t, badge)))
        .into_any_element()
}

pub(super) fn cell(t: &Theme, slot: &GitSlot) -> AnyElement {
    match slot.badge {
        Some(badge) => button(t, badge, slot),
        None => frame(t).into_any_element(),
    }
}

pub(super) fn header_cell(t: &Theme) -> impl IntoElement + use<> {
    frame(t)
        .id("git-header")
        .role(Role::ColumnHeader)
        .aria_label(tr("git-column"))
        .test_support()
        .child(icon(IconName::GitBranch, px(12.), t.git.quiet))
}

pub(super) fn counts_label(t: &Theme, text: String) -> impl IntoElement + use<> {
    div()
        .id("git-counts")
        .test_support()
        .flex_none()
        .font_family(t.typography.numbers_font.clone())
        .font_features(t.typography.numbers_font_features.clone())
        .font_weight(FontWeight::NORMAL)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}
