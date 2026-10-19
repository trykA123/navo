use gpui_kit::{AnyWindowHandle, Modifiers, Subscription, Task};

use super::FerriteWindow;
use crate::commands::{CommandSpec, active};
use crate::fkey_bar::KeyLayer;
use crate::modifier_hints::{
    Arm, Combo, Cover, Delay, HintsDrawer, HintsMode, Hold, Layer, drawer_height, layer_where,
};
use crate::prelude::*;
use crate::settings::{Change, Field};
use crate::tokens;

const MODIFIER_KEYS: [&str; 6] = [
    "control", "shift", "alt", "platform", "function", "capslock",
];

pub(super) struct Hints {
    pub(super) held: Option<KeyLayer>,
    hold: Hold,
    timer: Option<Task<()>>,
    _keys: Subscription,
}

impl Hints {
    pub(super) fn new(window: &Window, cx: &mut Context<FerriteWindow>) -> Self {
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
            timer: None,
            _keys: keys,
        }
    }
}

impl FerriteWindow {
    fn hints_allowed(&self, window: &Window) -> bool {
        let pane = self.browser.state().focused_pane();
        let in_list = self
            .focus
            .get(&pane)
            .is_some_and(|focus| focus.is_focused(window));
        self.values.modifier_hints() == HintsMode::All && self.viewer.is_none() && in_list
    }

    pub(super) fn hint_modifiers(
        &mut self,
        modifiers: Modifiers,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
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
            .filter(|_| self.hints_allowed(window))?;
        let focus = self.focus.get(&self.browser.state().focused_pane())?;
        let is_handled = |spec: &CommandSpec| {
            spec.action
                .and_then(|name| cx.build_action(name, None).ok())
                .is_some_and(|action| window.is_action_available_in(action.as_ref(), focus))
        };
        Some(layer_where(active(cx), combo, &is_handled))
    }

    pub(super) fn hint_cover(&self, window: &Window, cx: &App) -> Option<Cover> {
        let layer = self.hint_layer(window, cx)?;
        let t = tokens::current(cx);
        let bottom = window.viewport_size().height - self.fkey_strip_height(t);
        Some(Cover {
            top: bottom - drawer_height(t, &layer),
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
        let this = cx.weak_entity();
        let drawer = HintsDrawer::new(layer)
            .settings_key(key.as_deref())
            .on_turn_off(move |window, cx| {
                let _ = this.update(cx, |view, cx| view.turn_off_hints(window, cx));
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
