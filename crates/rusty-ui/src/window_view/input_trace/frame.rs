use gpui_kit::{
    App, DispatchPhase, FileDropEvent, IntoElement, MouseDownEvent, MouseEvent, MouseExitEvent,
    MouseMoveEvent, MousePressureEvent, MouseUpEvent, PinchEvent, ScrollWheelEvent, Styled, Window,
    canvas, deferred, div,
};

use super::State;
use crate::prelude::*;

pub(in crate::window_view) fn frame(window: &Window, cx: &App) -> Option<impl IntoElement + use<>> {
    let state = cx.try_global::<State>()?.clone();
    let started = cx.background_executor().now();
    let id = window.window_handle().window_id();
    let finish = deferred(
        canvas(
            |_, _, _| (),
            move |_, (), _, cx| {
                cx.defer(move |cx| state.finish(id, cx.background_executor().now() - started));
            },
        )
        .absolute()
        .size_0(),
    )
    .priority(usize::MAX);
    let events = canvas(
        |_, _, _| (),
        |_, (), window, _| {
            listen::<MouseDownEvent>("mouse:down", window);
            listen::<MouseUpEvent>("mouse:up", window);
            listen::<MouseMoveEvent>("mouse:move", window);
            listen::<ScrollWheelEvent>("mouse:scroll", window);
            listen::<MousePressureEvent>("mouse:pressure", window);
            listen::<PinchEvent>("mouse:pinch", window);
            listen::<MouseExitEvent>("mouse:exit", window);
            listen::<FileDropEvent>("mouse:drop", window);
        },
    );
    Some(div().absolute().size_0().child(events).child(finish))
}

fn listen<Event: MouseEvent>(action: &'static str, window: &mut Window) {
    window.on_mouse_event(move |_: &Event, phase, window, cx| {
        if phase == DispatchPhase::Capture {
            let state = cx.global::<State>().clone();
            state.begin(action.to_owned(), window, cx);
        }
    });
}
