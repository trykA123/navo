mod frame;
mod motion;
mod state;
mod view;

#[cfg(test)]
mod tests;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, App, DragMoveEvent, ElementId, Entity, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled, Window, div,
};

pub use frame::{SheetFrame, Side};
pub use state::{SheetState, Step};

use motion::Look;
use state::Phase;
use view::DraggedGrip;

use crate::tokens::{self, Theme};

gpui_kit::actions!(side_sheet, [Narrower, Wider]);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "side_sheet");
}

pub fn on_resize_keys<E: InteractiveElement>(
    element: E,
    state: &Entity<SheetState>,
    frame: SheetFrame,
) -> E {
    let (narrower, wider) = (state.clone(), state.clone());
    element
        .on_action(move |_: &Narrower, _, cx| step_open(&narrower, Step::Narrower, &frame, cx))
        .on_action(move |_: &Wider, _, cx| step_open(&wider, Step::Wider, &frame, cx))
}

fn step_open(state: &Entity<SheetState>, step: Step, frame: &SheetFrame, cx: &mut App) {
    if !state.read(cx).is_open() {
        cx.propagate();
        return;
    }
    state.update(cx, |state, cx| {
        state.step(step, frame);
        cx.notify();
    });
}

type CloseHandler = Rc<dyn Fn(&mut Window, &mut App)>;

struct Hint {
    keys: Vec<SharedString>,
    label: SharedString,
}

#[derive(IntoElement)]
pub struct SideSheet {
    id: ElementId,
    label: SharedString,
    state: Entity<SheetState>,
    frame: SheetFrame,
    title: Vec<AnyElement>,
    body: Vec<AnyElement>,
    hints: Vec<Hint>,
    close: Option<(SharedString, CloseHandler)>,
    is_ringed: bool,
}

impl SideSheet {
    pub fn new(id: impl Into<ElementId>, state: &Entity<SheetState>, frame: SheetFrame) -> Self {
        Self {
            id: id.into(),
            label: SharedString::default(),
            state: state.clone(),
            frame,
            title: Vec::new(),
            body: Vec::new(),
            hints: Vec::new(),
            close: None,
            is_ringed: false,
        }
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    #[must_use]
    pub fn header(mut self, title: impl IntoElement) -> Self {
        self.title.push(title.into_any_element());
        self
    }

    #[must_use]
    pub fn body(mut self, content: impl IntoElement) -> Self {
        self.body.push(content.into_any_element());
        self
    }

    #[must_use]
    pub fn hint(mut self, keys: &[&str], label: impl Into<SharedString>) -> Self {
        self.hints.push(Hint {
            keys: keys
                .iter()
                .map(|key| SharedString::from((*key).to_owned()))
                .collect(),
            label: label.into(),
        });
        self
    }

    #[must_use]
    pub fn close(
        mut self,
        key: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.close = Some((key.into(), Rc::new(handler)));
        self
    }

    #[must_use]
    pub fn accent_ring(mut self, is_ringed: bool) -> Self {
        self.is_ringed = is_ringed;
        self
    }
}

fn entered_at(sheet: &SideSheet, opened: usize, window: &mut Window, cx: &mut App) -> Instant {
    let name = match sheet.frame.side() {
        Side::Left => format!("enter-left-{opened}"),
        Side::Right => format!("enter-right-{opened}"),
    };
    let key = ElementId::NamedChild(Arc::new(sheet.id.clone()), name.into());
    let since = window.use_keyed_state(key, cx, |_, cx| cx.background_executor().now());
    *since.read(cx)
}

fn look(sheet: &SideSheet, window: &mut Window, cx: &mut App) -> Option<Look> {
    let (phase, opened) = {
        let state = sheet.state.read(cx);
        (state.phase(), state.opened())
    };
    let now = cx.background_executor().now();
    let reduce = cx.reduce_motion();
    let speed = tokens::current(cx).motion;
    match phase {
        Phase::Closed => None,
        Phase::Open => {
            let since = entered_at(sheet, opened, window, cx);
            Some(motion::entering(
                motion::elapsed(since, now, speed.slow),
                reduce,
            ))
        }
        Phase::Leaving(since) => {
            let look = motion::leaving(motion::elapsed(since, now, speed.base), reduce);
            (!look.is_settled).then_some(look)
        }
    }
}

fn place(t: &Theme, side: Side, look: Look) -> gpui_kit::Div {
    let edge = -look.outward;
    let sheet = div().absolute().top_0().bottom_0();
    match side {
        Side::Right => sheet.right(edge),
        Side::Left => sheet.left(edge),
    }
    .opacity(look.opacity)
    .rounded(t.radii.sheet)
    .bg(t.surfaces.sheet)
}

impl RenderOnce for SideSheet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(look) = look(&self, window, cx) else {
            return div().into_any_element();
        };
        if !look.is_settled {
            window.request_animation_frame();
        }
        let t = tokens::current(cx);
        let side = self.frame.side();
        let width = self.state.read(cx).width(&self.frame);
        let owner = self.state.entity_id();
        let (state, frame) = (self.state.clone(), self.frame);
        let shadow = if self.is_ringed {
            t.shadows.sheet_ring.clone()
        } else {
            t.shadows.sheet.clone()
        };
        let sheet = place(t, side, look)
            .id(self.id)
            .role(Role::Complementary)
            .aria_label(self.label)
            .aria_selected(self.is_ringed)
            .test_support()
            .occlude()
            .w(width)
            .flex()
            .flex_col()
            .shadow(shadow)
            .text_color(t.colors.text)
            .on_drag_move(move |event: &DragMoveEvent<DraggedGrip>, _, cx| {
                if event.drag(cx).0 != owner {
                    return;
                }
                let (x, bounds) = (event.event.position.x, event.bounds);
                let wanted = match side {
                    Side::Right => bounds.right() - x,
                    Side::Left => x - bounds.left(),
                };
                state.update(cx, |state, cx| {
                    state.resize_to(wanted, &frame);
                    cx.notify();
                });
            })
            .child(view::header(t, self.title, self.close))
            .child(view::body(self.body))
            .children(view::footer(t, self.hints))
            .child(view::grip(t, side, owner));
        sheet.into_any_element()
    }
}
