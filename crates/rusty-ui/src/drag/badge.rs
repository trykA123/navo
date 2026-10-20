use gpui_kit::{Context, IntoElement, Render, Window};

use super::{DragState, Mark};
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens;
use rusty_core::DropOp;

pub struct DragBadge {
    state: Entity<DragState>,
}

impl DragBadge {
    pub fn new(state: Entity<DragState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

fn icon_of(mark: Mark) -> IconName {
    match mark {
        Mark::Carry | Mark::Op(DropOp::Move) => IconName::FolderInput,
        Mark::Op(DropOp::Copy) => IconName::Copy,
        Mark::Op(DropOp::Link) => IconName::Link,
        Mark::Refused => IconName::Ban,
    }
}

impl Render for DragBadge {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx);
        let lines = self.state.read(cx).lines();
        let refused = lines.mark == Mark::Refused;
        let (color, edge) = if refused {
            (t.colors.muted, t.colors.line)
        } else {
            (t.colors.text, t.surfaces.card_ring)
        };
        div()
            .flex()
            .items_start()
            .gap(t.spacing.sm)
            .px(t.spacing.md)
            .py(t.spacing.sm)
            .rounded(t.radii.card)
            .bg(t.surfaces.card)
            .border_1()
            .border_color(edge)
            .shadow(t.shadows.menu.clone())
            .text_size(t.typography.body)
            .text_color(color)
            .when(refused, |badge| badge.opacity(0.85))
            .child(icon(icon_of(lines.mark), t.spacing.icon, color))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.xxs)
                    .child(div().font_weight(FontWeight::MEDIUM).child(lines.head))
                    .children(lines.keys.map(|keys| {
                        div()
                            .text_size(t.typography.small)
                            .text_color(t.colors.muted)
                            .child(keys)
                    })),
            )
    }
}
