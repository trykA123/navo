use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    App, DispatchPhase, Div, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Styled, Window, canvas, div, px,
};
use rusty_core::{Command, CursorMove};

use super::ListContext;
use crate::tokens::{self, Theme};

const THRESHOLD: f64 = 4.;
const FILL_ALPHA: f32 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    Row(u64),
    Below,
}

#[derive(Clone, Copy, Debug)]
struct Lasso {
    from: Point<Pixels>,
    to: Point<Pixels>,
    start: Start,
    unmark: bool,
    is_plain: bool,
    is_drawn: bool,
    far: Option<u64>,
}

#[derive(Default)]
pub(super) struct Held {
    lasso: Option<Lasso>,
    swallows: bool,
}

#[derive(Clone, Copy, Debug)]
struct Rows {
    top: Pixels,
    height: Pixels,
    len: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Still,
    Redraw,
    Span { anchor: Option<u64>, far: u64 },
}

impl Rows {
    fn row_at(&self, y: Pixels) -> i64 {
        ((y - self.top) / self.height).floor() as i64
    }

    fn bottom(&self) -> Pixels {
        self.top + self.height * self.len as f32
    }

    fn crossed(&self, a: Pixels, b: Pixels) -> Option<(u64, u64)> {
        let last = self.len.checked_sub(1)?;
        let first = u64::try_from(self.row_at(a.min(b)).max(0)).ok()?;
        let end = u64::try_from(self.row_at(a.max(b))).ok()?;
        (first <= last).then_some((first, end.min(last)))
    }
}

impl Held {
    pub(super) fn press(&mut self) {
        self.swallows = false;
    }

    pub(super) fn swallows_click(&self) -> bool {
        self.swallows
    }

    fn arm(&mut self, start: Start, event: &MouseDownEvent) {
        let modifiers = event.modifiers;
        self.swallows = false;
        self.lasso = Some(Lasso {
            from: event.position,
            to: event.position,
            start,
            unmark: modifiers.control,
            is_plain: !modifiers.modified(),
            is_drawn: false,
            far: None,
        });
    }

    fn track(&mut self, at: Point<Pixels>, rows: Rows) -> Step {
        let Some(lasso) = self.lasso.as_mut() else {
            return Step::Still;
        };
        lasso.to = at;
        if !lasso.is_drawn {
            if (at - lasso.from).magnitude() <= THRESHOLD {
                return Step::Still;
            }
            lasso.is_drawn = true;
            self.swallows = true;
        }
        let Some(anchor) = (match lasso.start {
            Start::Row(row) => Some(row),
            Start::Below => rows.len.checked_sub(1),
        }) else {
            return Step::Redraw;
        };
        let far = match rows.crossed(lasso.from.y, at.y) {
            Some((first, _)) if at.y < lasso.from.y => first,
            Some((_, end)) => end,
            None if lasso.far.is_some() => anchor,
            None => return Step::Redraw,
        };
        let step = Step::Span {
            anchor: lasso.far.is_none().then_some(anchor),
            far,
        };
        if lasso.far.replace(far) == Some(far) {
            return Step::Redraw;
        }
        step
    }
}

impl ListContext {
    fn rows(&self, cx: &App) -> Option<Rows> {
        let state = self.state.read(cx);
        let list = state.list_bounds?;
        Some(Rows {
            top: list.origin.y - state.lift + state.scroll.offset_y(),
            height: tokens::current(cx).rows.height,
            len: super::row_number(self.props.rows.len()),
        })
    }

    fn lasso_to(&self, at: Point<Pixels>, window: &mut Window, cx: &mut App) {
        let Some(rows) = self.rows(cx) else {
            return;
        };
        let step = self
            .state
            .update(cx, |state, _| state.lasso.track(at, rows));
        let Step::Span { anchor, far } = step else {
            if step == Step::Redraw {
                window.refresh();
            }
            return;
        };
        let unmark = self.state.read(cx).lasso.lasso.is_some_and(|l| l.unmark);
        if let Some(anchor) = anchor {
            let to = CursorMove::Row(anchor);
            self.send(Command::PaneCursorMove { to }, window, cx);
        }
        let to = CursorMove::Row(far);
        self.send(Command::SelectionExtend { to, unmark }, window, cx);
        window.refresh();
    }

    fn lasso_up(&self, window: &mut Window, cx: &mut App) {
        let Some(lasso) = self.state.update(cx, |state, _| state.lasso.lasso.take()) else {
            return;
        };
        if lasso.is_drawn {
            window.refresh();
        } else if lasso.start == Start::Below && lasso.is_plain {
            self.send(Command::SelectionClear, window, cx);
        }
    }
}

pub(super) fn layer(area: Div, ctx: &Rc<ListContext>, t: &Theme, cx: &App) -> Div {
    area.on_mouse_down(MouseButton::Left, below_down(ctx))
        .relative()
        .child(watch(ctx))
        .children(rect(ctx, t, cx))
}

pub(super) fn start_zone(ctx: &Rc<ListContext>, row: u64) -> Div {
    let ctx = ctx.clone();
    div().on_mouse_down(MouseButton::Left, move |event, _, cx| {
        let start = Start::Row(row);
        ctx.state
            .update(cx, |state, _| state.lasso.arm(start, event));
    })
}

fn below_down(ctx: &Rc<ListContext>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |event, window, cx| {
        let is_below = ctx
            .rows(cx)
            .is_some_and(|rows| event.position.y >= rows.bottom());
        if is_below {
            ctx.take_focus(window, cx);
            ctx.state
                .update(cx, |state, _| state.lasso.arm(Start::Below, event));
        }
    }
}

fn watch(ctx: &Rc<ListContext>) -> impl IntoElement {
    let ctx = ctx.clone();
    canvas(
        |_, _, _| {},
        move |_, (), window, _| {
            let (moved, released) = (ctx.clone(), ctx);
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                if event.pressed_button == Some(MouseButton::Left) {
                    moved.lasso_to(event.position, window, cx);
                } else {
                    moved.lasso_up(window, cx);
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    released.lasso_up(window, cx);
                }
            });
        },
    )
    .absolute()
    .size_0()
}

fn rect(ctx: &ListContext, t: &Theme, cx: &App) -> Option<impl IntoElement + use<>> {
    let state = ctx.state.read(cx);
    let lasso = state.lasso.lasso.filter(|lasso| lasso.is_drawn)?;
    let origin = state.list_bounds?.origin;
    let (from, to) = (lasso.from, lasso.to);
    let accent = t.colors.accent;
    Some(
        div()
            .id("lasso")
            .test_support()
            .absolute()
            .left(from.x.min(to.x) - origin.x)
            .top(from.y.min(to.y) - origin.y)
            .w((from.x - to.x).abs())
            .h((from.y - to.y).abs())
            .bg(accent.opacity(FILL_ALPHA))
            .border(px(1.))
            .border_color(accent),
    )
}

#[cfg(test)]
mod tests {
    use super::{Rows, px};

    #[test]
    fn rows_crossed_by_a_band_clamp_to_the_listing() {
        let rows = Rows {
            top: px(100.),
            height: px(20.),
            len: 5,
        };
        assert_eq!(rows.crossed(px(130.), px(175.)), Some((1, 3)));
        assert_eq!(rows.crossed(px(260.), px(150.)), Some((2, 4)));
        assert_eq!(rows.crossed(px(40.), px(90.)), None);
        assert_eq!(rows.crossed(px(210.), px(240.)), None);
        assert_eq!(rows.crossed(px(40.), px(105.)), Some((0, 0)));
    }
}
