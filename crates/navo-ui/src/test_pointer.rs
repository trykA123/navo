use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, PlatformInput, Point, TestAppContext,
};

pub(crate) fn dispatch(cx: &mut TestAppContext, window: AnyWindowHandle, input: PlatformInput) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_event(input, cx);
    })
    .expect("window");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

pub(crate) fn down(
    button: MouseButton,
    position: Point<Pixels>,
    modifiers: Modifiers,
) -> PlatformInput {
    MouseDownEvent {
        button,
        position,
        modifiers,
        click_count: 1,
        first_mouse: false,
    }
    .to_platform_input()
}

pub(crate) fn up(
    button: MouseButton,
    position: Point<Pixels>,
    modifiers: Modifiers,
) -> PlatformInput {
    MouseUpEvent {
        button,
        position,
        modifiers,
        click_count: 1,
    }
    .to_platform_input()
}

pub(crate) fn moved(
    pressed_button: Option<MouseButton>,
    position: Point<Pixels>,
    modifiers: Modifiers,
) -> PlatformInput {
    MouseMoveEvent {
        position,
        pressed_button,
        modifiers,
    }
    .to_platform_input()
}

pub(crate) fn click(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    (at, modifiers): (Point<Pixels>, Modifiers),
) {
    dispatch(cx, window, down(MouseButton::Left, at, modifiers));
    dispatch(cx, window, up(MouseButton::Left, at, modifiers));
}

pub(crate) fn drag(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    (button, modifiers): (MouseButton, Modifiers),
    path: &[Point<Pixels>],
) {
    let [from, rest @ ..] = path else {
        return;
    };
    dispatch(cx, window, down(button, *from, modifiers));
    for at in rest {
        dispatch(cx, window, moved(Some(button), *at, modifiers));
    }
}

pub(crate) fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Modifiers::default()
    }
}

pub(crate) fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Modifiers::default()
    }
}
