//! Resizable two-child split container (horizontal or vertical) with double-click reset.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, DragMoveEvent, ElementId, Empty, Entity,
    EntityId, InteractiveElement, IntoElement, ParentElement, Pixels, Point, Render, RenderOnce,
    Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px, relative,
    transparent_black,
};

use super::Tooltip;
use crate::tokens;

const MIN_RATIO: f32 = 0.1;
const MAX_RATIO: f32 = 0.9;
const HANDLE_WIDTH: f32 = 8.;

type OnChange = Rc<dyn Fn(f32, &mut Window, &mut App)>;

fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_nan() {
        return 0.5;
    }
    ratio.clamp(MIN_RATIO, MAX_RATIO)
}

/// Direction a [`Splitter`] divides its two children along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitterAxis {
    /// Side-by-side split: first on the left, second on the right.
    #[default]
    Horizontal,
    /// Stacked split: first on top, second on the bottom.
    Vertical,
}

/// Persistent split ratio state shared by a [`Splitter`] across renders.
pub struct SplitterState {
    ratio: f32,
    default_ratio: f32,
}

impl SplitterState {
    /// Creates a splitter starting at `ratio` with `0.5` as the double-click reset target.
    pub fn new(ratio: f32) -> Self {
        Self::with_default(ratio, 0.5)
    }

    /// Creates a splitter starting at `ratio` with `default_ratio` as the double-click reset target.
    pub fn with_default(ratio: f32, default_ratio: f32) -> Self {
        Self {
            ratio: clamp_ratio(ratio),
            default_ratio: clamp_ratio(default_ratio),
        }
    }

    /// Current split ratio assigned to the first child (`0.1..=0.9`).
    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Updates the split ratio, clamping it to `0.1..=0.9`.
    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = clamp_ratio(ratio);
    }

    /// Resets the split ratio to its configured default.
    pub fn reset(&mut self) {
        self.ratio = self.default_ratio;
    }

    fn drag_to(
        &mut self,
        pos: Point<Pixels>,
        bounds: Bounds<Pixels>,
        axis: SplitterAxis,
        min_second: Option<Pixels>,
    ) {
        let (offset, span) = match axis {
            SplitterAxis::Horizontal => (pos.x - bounds.origin.x, bounds.size.width),
            SplitterAxis::Vertical => (pos.y - bounds.origin.y, bounds.size.height),
        };
        if span <= px(0.) {
            return;
        }
        let mut next = clamp_ratio(offset / span);
        if let Some(min_second) = min_second {
            let max_first = (1.0 - (min_second / span)).max(MIN_RATIO);
            next = next.min(max_first);
        }
        self.set_ratio(next);
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

/// Interactive split container dividing two children along a horizontal or vertical seam.
#[derive(IntoElement)]
pub struct Splitter {
    id: ElementId,
    state: Entity<SplitterState>,
    first: AnyElement,
    second: AnyElement,
    axis: SplitterAxis,
    min_second: Option<Pixels>,
    is_line_on_hover: bool,
    aria_label: Option<SharedString>,
    tooltip_text: Option<SharedString>,
    on_change: Option<OnChange>,
}

impl Splitter {
    /// Creates a horizontal splitter between `first` (left) and `second` (right).
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
            axis: SplitterAxis::Horizontal,
            min_second: None,
            is_line_on_hover: false,
            aria_label: None,
            tooltip_text: None,
            on_change: None,
        }
    }

    /// Switches this splitter to a vertical stack (`first` on top, `second` on the bottom).
    #[must_use]
    pub fn vertical(mut self) -> Self {
        self.axis = SplitterAxis::Vertical;
        self
    }

    /// Enforces a minimum logical pixel size on the second child.
    #[must_use]
    pub fn min_second(mut self, min_size: Pixels) -> Self {
        self.min_second = Some(min_size);
        self
    }

    /// Hides the divider line until the handle is hovered.
    #[must_use]
    pub fn line_on_hover(mut self) -> Self {
        self.is_line_on_hover = true;
        self
    }

    /// Overrides the accessible label on the resize handle.
    #[must_use]
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    /// Overrides the tooltip shown on the resize handle.
    #[must_use]
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip_text = Some(text.into());
        self
    }

    /// Registers a callback invoked whenever dragging or double-click reset changes the ratio.
    #[must_use]
    pub fn on_change(mut self, callback: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(callback));
        self
    }
}

struct HandleLook {
    ratio: f32,
    axis: SplitterAxis,
    state: Entity<SplitterState>,
    owner: EntityId,
    is_line_on_hover: bool,
    aria_label: SharedString,
    tooltip_text: SharedString,
    on_change: Option<OnChange>,
}

fn handle(look: HandleLook, cx: &App) -> impl IntoElement {
    let HandleLook {
        ratio,
        axis,
        state,
        owner,
        is_line_on_hover,
        aria_label,
        tooltip_text,
        on_change,
    } = look;
    let t = tokens::current(cx);
    let accent = t.colors.accent;
    let line = if is_line_on_hover {
        transparent_black()
    } else {
        t.colors.line
    };
    let base = div()
        .id("handle")
        .role(Role::Splitter)
        .aria_label(aria_label)
        .test_support()
        .group("splitter-handle")
        .absolute()
        .flex()
        .tooltip(move |window, cx| Tooltip::new(tooltip_text.clone()).build(window, cx))
        .on_click(move |event, window, cx| {
            if event.click_count() >= 2 {
                let next = state.update(cx, |state, cx| {
                    state.reset();
                    cx.notify();
                    state.ratio()
                });
                if let Some(callback) = &on_change {
                    callback(next, window, cx);
                }
            }
        })
        .on_drag(DraggedSplitter(owner), |_, _, _, cx| cx.new(|_| DragGhost));

    match axis {
        SplitterAxis::Horizontal => base
            .top_0()
            .bottom_0()
            .left(relative(ratio))
            .ml(px(-HANDLE_WIDTH / 2.))
            .w(px(HANDLE_WIDTH))
            .justify_center()
            .cursor_col_resize()
            .child(
                div()
                    .w(px(1.))
                    .h_full()
                    .bg(line)
                    .group_hover("splitter-handle", move |style| style.bg(accent)),
            ),
        SplitterAxis::Vertical => base
            .left_0()
            .right_0()
            .top(px(-HANDLE_WIDTH / 2.))
            .h(px(HANDLE_WIDTH))
            .items_center()
            .cursor_row_resize()
            .child(
                div()
                    .h(px(1.))
                    .w_full()
                    .bg(line)
                    .group_hover("splitter-handle", move |style| style.bg(accent)),
            ),
    }
}

impl RenderOnce for Splitter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ratio = self.state.read(cx).ratio();
        let owner = self.state.entity_id();
        let axis = self.axis;
        let min_second = self.min_second;
        let state = self.state;
        let handle_state = state.clone();
        let on_drag_change = self.on_change.clone();
        let handle_el = handle(
            HandleLook {
                ratio,
                axis,
                state: handle_state,
                owner,
                is_line_on_hover: self.is_line_on_hover,
                aria_label: self
                    .aria_label
                    .unwrap_or_else(|| SharedString::new_static("Resize panes")),
                tooltip_text: self
                    .tooltip_text
                    .unwrap_or_else(|| SharedString::new_static("Double-click to centre")),
                on_change: self.on_change,
            },
            cx,
        );
        let root = div()
            .id(self.id)
            .test_support()
            .relative()
            .flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .on_drag_move(move |event: &DragMoveEvent<DraggedSplitter>, window, cx| {
                if event.drag(cx).0 != owner {
                    return;
                }
                let pos = event.event.position;
                let bounds = event.bounds;
                let next = state.update(cx, |state, cx| {
                    state.drag_to(pos, bounds, axis, min_second);
                    cx.notify();
                    state.ratio()
                });
                if let Some(callback) = &on_drag_change {
                    callback(next, window, cx);
                }
            });

        assemble_split(
            root,
            SplitParts {
                axis,
                ratio,
                min_second,
                first: self.first,
                second: self.second,
                handle: handle_el.into_any_element(),
            },
        )
    }
}

struct SplitParts {
    axis: SplitterAxis,
    ratio: f32,
    min_second: Option<Pixels>,
    first: AnyElement,
    second: AnyElement,
    handle: AnyElement,
}

fn assemble_split(
    root: impl ParentElement + Styled + IntoElement,
    parts: SplitParts,
) -> impl IntoElement {
    match parts.axis {
        SplitterAxis::Horizontal => root
            .child(
                div()
                    .h_full()
                    .w(relative(parts.ratio))
                    .flex_none()
                    .overflow_hidden()
                    .child(parts.first),
            )
            .child(
                div()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(parts.second),
            )
            .child(parts.handle),
        SplitterAxis::Vertical => {
            let mut bottom = div()
                .relative()
                .w_full()
                .h(relative(1.0 - parts.ratio))
                .flex_none();
            if let Some(min_h) = parts.min_second {
                bottom = bottom.min_h(min_h);
            }
            root.flex_col()
                .child(
                    div()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(parts.first),
                )
                .child(
                    bottom
                        .child(div().size_full().overflow_hidden().child(parts.second))
                        .child(parts.handle),
                )
        }
    }
}
