use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, DragMoveEvent, ElementId, Empty, Entity,
    EntityId, InteractiveElement, IntoElement, ParentElement, Pixels, Render, RenderOnce, Role,
    StatefulInteractiveElement, Styled, Window, div, px, relative,
};

use crate::tokens;

const MIN_RATIO: f32 = 0.1;
const MAX_RATIO: f32 = 0.9;
const HANDLE_WIDTH: f32 = 8.;

fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_nan() {
        return 0.5;
    }
    ratio.clamp(MIN_RATIO, MAX_RATIO)
}

pub struct SplitterState {
    ratio: f32,
}

impl SplitterState {
    pub fn new(ratio: f32) -> Self {
        Self {
            ratio: clamp_ratio(ratio),
        }
    }

    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = clamp_ratio(ratio);
    }

    fn drag_to(&mut self, x: Pixels, bounds: Bounds<Pixels>) {
        let width = bounds.size.width;
        if width <= px(0.) {
            return;
        }
        self.set_ratio((x - bounds.origin.x) / width);
    }
}

#[derive(Clone, Copy)]
struct DraggedSplitter(EntityId);

struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(IntoElement)]
pub struct Splitter {
    id: ElementId,
    state: Entity<SplitterState>,
    first: AnyElement,
    second: AnyElement,
}

impl Splitter {
    pub fn new(
        id: impl Into<ElementId>,
        state: &Entity<SplitterState>,
        first: impl IntoElement,
        second: impl IntoElement,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            first: first.into_any_element(),
            second: second.into_any_element(),
        }
    }
}

fn handle(ratio: f32, owner: EntityId, cx: &App) -> impl IntoElement {
    let t = tokens::current(cx);
    let (line, accent) = (t.colors.line, t.colors.accent);
    div()
        .id("handle")
        .role(Role::Splitter)
        .aria_label("Resize panes")
        .test_support()
        .group("splitter-handle")
        .absolute()
        .top_0()
        .bottom_0()
        .left(relative(ratio))
        .ml(px(-HANDLE_WIDTH / 2.))
        .w(px(HANDLE_WIDTH))
        .flex()
        .justify_center()
        .cursor_col_resize()
        .on_drag(DraggedSplitter(owner), |_, _, _, cx| cx.new(|_| DragGhost))
        .child(
            div()
                .w(px(1.))
                .h_full()
                .bg(line)
                .group_hover("splitter-handle", move |style| style.bg(accent)),
        )
}

impl RenderOnce for Splitter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ratio = self.state.read(cx).ratio();
        let owner = self.state.entity_id();
        let state = self.state;
        div()
            .id(self.id)
            .test_support()
            .relative()
            .flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .on_drag_move(move |event: &DragMoveEvent<DraggedSplitter>, _, cx| {
                if event.drag(cx).0 != owner {
                    return;
                }
                let x = event.event.position.x;
                let bounds = event.bounds;
                state.update(cx, |state, cx| {
                    state.drag_to(x, bounds);
                    cx.notify();
                });
            })
            .child(
                div()
                    .h_full()
                    .w(relative(ratio))
                    .flex_none()
                    .overflow_hidden()
                    .child(self.first),
            )
            .child(
                div()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.second),
            )
            .child(handle(ratio, owner, cx))
    }
}
