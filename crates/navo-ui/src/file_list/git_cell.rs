use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Div, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Role, StatefulInteractiveElement, Styled, div, px,
};
use navo_fs::{EntryKind, FsEntry};

use super::{FileListAction, ListContext, git_text};
use crate::git::{BadgeKind, RepoBadge};
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::Theme;

const STALE_OPACITY: f32 = 0.5;
const DOT: Pixels = px(4.);
/// Glyph box in the corner of the folder icon. The ring adds 1 px on each side.
const GLYPH: Pixels = px(10.);
const RING: Pixels = px(1.);
const CORNER: Pixels = px(-4.);

/// What a row needs to draw the mark on its folder icon.
#[derive(Clone, Copy)]
pub(super) struct GitSlot<'a> {
    pub badge: &'a RepoBadge,
    pub ctx: &'a Rc<ListContext>,
    pub row: u64,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Mark {
    Glyph(IconName, Hsla),
    Dot(Hsla),
}

pub(super) fn slot<'a>(
    ctx: &'a Rc<ListContext>,
    (entry, row): (&FsEntry, u64),
) -> Option<GitSlot<'a>> {
    if ctx.find_results.is_some() || entry.kind != EntryKind::Folder {
        return None;
    }
    let badge = ctx.props.git.as_ref()?.by_name.get(&entry.name.to_text())?;
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
        Mark::Glyph(name, color) => icon(name, GLYPH, color).into_any_element(),
        Mark::Dot(color) => div()
            .size(DOT)
            .rounded(t.radii.full)
            .bg(color)
            .into_any_element(),
    }
}

/// The mark in the bottom-right corner of its parent, which must be `relative`. `ground` is
/// the row background, drawn as a 1 px ring so the mark reads on the icon.
fn corner(t: &Theme, badge: &RepoBadge, ground: Hsla) -> Div {
    div()
        .absolute()
        .right(CORNER)
        .bottom(CORNER)
        .size(GLYPH + RING * 2.)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(RING)
                .rounded(t.radii.full)
                .bg(ground)
                .child(
                    div()
                        .when(badge.stale, |d| d.opacity(STALE_OPACITY))
                        .child(mark_element(t, mark(t, badge))),
                ),
        )
}

/// An icon with the state mark on its corner, as in a row. It does not react to the mouse.
pub fn marked_icon(t: &Theme, glyph: impl IntoElement, badge: &RepoBadge, ground: Hsla) -> Div {
    div()
        .relative()
        .flex_none()
        .child(glyph)
        .child(corner(t, badge, ground))
}

/// The mark of a row: opens the Git menu on click and shows the tooltip on hover.
pub(super) fn overlay(t: &Theme, slot: &GitSlot, ground: Hsla) -> AnyElement {
    let (ctx, row, badge) = (slot.ctx.clone(), slot.row, slot.badge);
    let tip = git_text::tooltip(badge);
    corner(t, badge, ground)
        .id("git-badge")
        .role(Role::Button)
        .aria_label(git_text::spoken(badge))
        .test_support()
        .cursor_pointer()
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            let at = event.position();
            ctx.act(FileListAction::GitMenu { row, at }, window, cx);
        })
        .into_any_element()
}
