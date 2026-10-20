use std::rc::Rc;

use gpui_kit::{App, DragMoveEvent, ExternalPaths, Hsla, InteractiveElement, Styled, Window};

use super::FileDrag;
use crate::file_list::DropPayload;
use crate::file_list::row::inset_ring;

type LandHandler<T> = Rc<dyn Fn(&T, &DropPayload, &mut Window, &mut App)>;
type OverHandler<T> = Rc<dyn Fn(&T, bool, &mut Window, &mut App)>;

pub struct Landing<T> {
    on_drop: Option<LandHandler<T>>,
    on_hover: Option<OverHandler<T>>,
}

impl<T> Clone for Landing<T> {
    fn clone(&self) -> Self {
        Self {
            on_drop: self.on_drop.clone(),
            on_hover: self.on_hover.clone(),
        }
    }
}

impl<T> Default for Landing<T> {
    fn default() -> Self {
        Self {
            on_drop: None,
            on_hover: None,
        }
    }
}

impl<T: Clone + 'static> Landing<T> {
    pub fn on_drop(&mut self, handler: impl Fn(&T, &DropPayload, &mut Window, &mut App) + 'static) {
        self.on_drop = Some(Rc::new(handler));
    }

    pub fn on_hover(&mut self, handler: impl Fn(&T, bool, &mut Window, &mut App) + 'static) {
        self.on_hover = Some(Rc::new(handler));
    }

    pub fn accept<E: InteractiveElement>(&self, el: E, (target, ring): (T, Hsla)) -> E {
        let Some(on_drop) = self.on_drop.clone() else {
            return el;
        };
        let el = el
            .drag_over::<FileDrag>(move |style, _, _, _| style.shadow(inset_ring(ring)))
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.shadow(inset_ring(ring)));
        let el = dropping::<FileDrag, _, _>(el, (&on_drop, &target), |drag| {
            DropPayload::Internal(*drag)
        });
        let el = dropping::<ExternalPaths, _, _>(el, (&on_drop, &target), |paths| {
            DropPayload::External(paths.clone())
        });
        let Some(on_hover) = self.on_hover.clone() else {
            return el;
        };
        hovering::<ExternalPaths, _, _>(
            hovering::<FileDrag, _, _>(el, (&on_hover, &target)),
            (&on_hover, &target),
        )
    }
}

fn dropping<V: 'static, T: Clone + 'static, E: InteractiveElement>(
    el: E,
    (handler, target): (&LandHandler<T>, &T),
    payload: impl Fn(&V) -> DropPayload + 'static,
) -> E {
    let (handler, target) = (handler.clone(), target.clone());
    el.on_drop(move |value: &V, window, cx| handler(&target, &payload(value), window, cx))
}

fn hovering<V: 'static, T: Clone + 'static, E: InteractiveElement>(
    el: E,
    (handler, target): (&OverHandler<T>, &T),
) -> E {
    let (handler, target) = (handler.clone(), target.clone());
    el.on_drag_move(move |event: &DragMoveEvent<V>, window, cx| {
        let inside = event.bounds.contains(&event.event.position);
        let (handler, target) = (handler.clone(), target.clone());
        window.defer(cx, move |window, cx| handler(&target, inside, window, cx));
    })
}
