//! Modifier hints in the window: tracks held modifiers and mouse buttons, and places
//! the ribbon card above the F-key bar when it fits.

use gpui_kit::{
    AnyWindowHandle, DispatchPhase, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, NavigationDirection, Subscription, Task, WeakEntity, canvas,
};

use super::NavoWindow;
use crate::commands::{CommandSpec, active};
use crate::fkey_bar::KeyLayer;
use crate::modifier_hints::{
    Arm, Combo, Cover, Delay, Fit, HintsDrawer, HintsMode, Hold, Layer, drawer_height, layer_where,
};
use crate::prelude::*;
use crate::settings::{Change, Field, WindowParts};
use crate::tokens;

const MODIFIER_KEYS: [&str; 6] = [
    "control", "shift", "alt", "platform", "function", "capslock",
];

pub(super) struct Hints {
    pub(super) held: Option<KeyLayer>,
    hold: Hold,
    buttons: Buttons,
    keeps_open: bool,
    timer: Option<Task<()>>,
    _keys: Subscription,
}

/// The mouse buttons held in the window, one bit per button.
#[derive(Clone, Copy, Default)]
struct Buttons(u8);

impl Buttons {
    fn bit(button: MouseButton) -> u8 {
        match button {
            MouseButton::Left => 1,
            MouseButton::Right => 2,
            MouseButton::Middle => 4,
            MouseButton::Navigate(NavigationDirection::Back) => 8,
            MouseButton::Navigate(NavigationDirection::Forward) => 16,
        }
    }

    fn is_any(self) -> bool {
        self.0 != 0
    }
}

impl Hints {
    pub(super) fn new(window: &Window, cx: &mut Context<NavoWindow>) -> Self {
        let this = cx.weak_entity();
        let own: AnyWindowHandle = window.window_handle();
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            let key = event.keystroke.key.as_str();
            if window.window_handle() == own && !MODIFIER_KEYS.contains(&key) {
                let _ = this.update(cx, |view, _| view.hints.hold.key_pressed());
            }
        });
        Self {
            held: None,
            hold: Hold::default(),
            buttons: Buttons::default(),
            keeps_open: false,
            timer: None,
            _keys: keys,
        }
    }
}

impl NavoWindow {
    /// True when hints are on, no viewer is open and the focused pane's list has focus.
    fn hints_fit(&self, window: &Window) -> bool {
        let pane = self.browser.state().focused_pane();
        let in_list = self
            .focus
            .get(&pane)
            .is_some_and(|focus| focus.is_focused(window));
        let is_on = self.values.modifier_hints() == HintsMode::All;
        is_on && self.viewer.is_none() && in_list
    }

    /// True when a drawer may arm or show: it fits and no mouse button is held.
    fn hints_allowed(&self, window: &Window) -> bool {
        self.hints_fit(window) && !self.hints.buttons.is_any()
    }

    /// Notes a pressed button. The drawer hides until every button is up, unless the press
    /// is on its Turn off button; the press itself still reaches the element under the pointer.
    fn hint_mouse_down(&mut self, button: MouseButton, cx: &mut Context<Self>) {
        let was_free = !self.hints.buttons.is_any();
        self.hints.buttons.0 |= Buttons::bit(button);
        if self.hints.hold.shown().is_none() {
            self.hints.hold.cancel();
        } else if was_free {
            cx.notify();
        }
    }

    /// Keeps the drawer through a press on its Turn off button, so the click can land.
    fn hint_turn_off_pressed(&mut self) {
        self.hints.keeps_open = true;
    }

    /// Notes a released button; the press ends once every button is up. A press on Turn off
    /// then catches up with the modifiers it ignored, after its click handlers were captured.
    fn hint_mouse_up(&mut self, button: MouseButton, window: &Window, cx: &mut Context<Self>) {
        self.hints.buttons.0 &= !Buttons::bit(button);
        if !self.hints.buttons.is_any() {
            let was_kept = self.hints.keeps_open;
            self.end_hint_press();
            if was_kept {
                self.hint_modifiers(window.modifiers(), window, cx);
            }
        }
    }

    /// Forgets every held button: a move with no button held or a change of activation
    /// shows that a release never reached the window.
    pub(super) fn hint_buttons_free(&mut self) {
        if self.hints.buttons.is_any() {
            self.hints.buttons = Buttons::default();
            self.end_hint_press();
        }
    }

    /// Ends a press: a drawer it hid stays closed until the modifiers are pressed again.
    /// Nothing on screen changes, so there is no redraw.
    fn end_hint_press(&mut self) {
        if !std::mem::take(&mut self.hints.keeps_open) {
            self.hints.hold.cancel();
        }
    }

    /// Watches every mouse button in the window, wherever the pointer is, in the capture phase.
    pub(super) fn button_watch(cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.weak_entity();
        canvas(
            |_, _, _| {},
            move |_, (), window, _| watch_buttons(&view, window),
        )
        .absolute()
        .size_0()
    }

    /// Arms, shows or hides the drawer for the held modifiers. Ignored while Turn off is
    /// pressed, so the button stays mounted until its release.
    pub(super) fn hint_modifiers(
        &mut self,
        modifiers: Modifiers,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.hints.keeps_open {
            return;
        }
        let held = Combo::of(&modifiers).filter(|_| window.is_window_active());
        let delay = Delay::from_ms(self.values.modifier_hints_delay_ms());
        let was = self.hints.hold.shown();
        let arm = self.hints.hold.modifiers(held, delay);
        if !self.hints_allowed(window) {
            self.hints.hold.cancel();
        } else if let Some(arm) = arm {
            self.arm_hint(arm, window, cx);
        }
        if was != self.hints.hold.shown() {
            cx.notify();
        }
    }

    fn arm_hint(&mut self, arm: Arm, window: &Window, cx: &mut Context<Self>) {
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(arm.delay).await;
            let _ = this.update_in(cx, |view, window, cx| view.hint_due(arm.token, window, cx));
        });
        self.hints.timer = Some(task);
    }

    fn hint_due(&mut self, token: u64, window: &Window, cx: &mut Context<Self>) {
        if !self.hints_allowed(window) {
            self.hints.hold.cancel();
        } else if self.hints.hold.elapsed(token) {
            cx.notify();
        }
    }

    fn hint_layer(&self, window: &Window, cx: &App) -> Option<Layer> {
        let combo = self
            .hints
            .hold
            .shown()
            .filter(|_| self.hints_fit(window) && !self.workspace_keys.visible)
            .filter(|_| !self.hints.buttons.is_any() || self.hints.keeps_open)?;
        let focus = self.focus.get(&self.browser.state().focused_pane())?;
        let is_handled = |spec: &CommandSpec| {
            spec.action
                .and_then(|name| cx.build_action(name, None).ok())
                .is_some_and(|action| window.is_action_available_in(action.as_ref(), focus))
        };
        Some(layer_where(active(cx), combo, &is_handled))
    }

    /// Where the drawer sits: the window's width and whether the F-key bar shows.
    fn hint_fit(&self, window: &Window, t: &tokens::Theme) -> Fit {
        let has_bar = WindowParts::of(&self.values).fkey_bar;
        Fit::in_window(t, window.viewport_size().width, has_bar)
    }

    pub(super) fn hint_cover(&self, window: &Window, cx: &App) -> Option<Cover> {
        let layer = self.hint_layer(window, cx)?;
        let t = tokens::current(cx);
        let bottom = window.viewport_size().height - self.fkey_strip_height(t);
        Some(Cover {
            top: bottom - drawer_height(t, &layer, self.hint_fit(window, t)),
            gap: t.spacing.md,
        })
    }

    pub(super) fn hints_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let layer = self.hint_layer(window, cx)?;
        let t = tokens::current(cx).clone();
        let key = active(cx)
            .keys_of("settings.toggle")
            .next()
            .map(str::to_owned);
        let (this, pressed) = (cx.weak_entity(), cx.weak_entity());
        let drawer = HintsDrawer::new(layer, self.hint_fit(window, &t))
            .settings_key(key.as_deref())
            .on_turn_off(move |window, cx| {
                let _ = this.update(cx, |view, cx| view.turn_off_hints(window, cx));
            })
            .on_turn_off_press(move |_, cx| {
                let _ = pressed.update(cx, |view, _| view.hint_turn_off_pressed());
            });
        Some(
            div()
                .absolute()
                .left(t.spacing.md)
                .right(t.spacing.md)
                .bottom_0()
                .child(drawer)
                .into_any_element(),
        )
    }

    fn turn_off_hints(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hints.hold.cancel();
        self.change_setting(Change::modifier_hints(HintsMode::Fkeys), window, cx);
        self.open_settings_at(Field::ModifierHints, window, cx);
        cx.notify();
    }
}

/// Registers this frame's capture-phase listeners for presses, releases and buttonless moves.
fn watch_buttons(view: &WeakEntity<NavoWindow>, window: &mut Window) {
    let (pressed, released, moved) = (view.clone(), view.clone(), view.clone());
    window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
        if phase == DispatchPhase::Capture {
            let _ = pressed.update(cx, |view, cx| view.hint_mouse_down(event.button, cx));
        }
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
        if phase == DispatchPhase::Capture {
            let _ = released.update(cx, |view, cx| view.hint_mouse_up(event.button, window, cx));
        }
    });
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
        if phase == DispatchPhase::Capture && event.pressed_button.is_none() {
            let _ = moved.update(cx, |view, _| view.hint_buttons_free());
        }
    });
}
