use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{RenderOnce, Role};

use crate::commands::Keymap;
use crate::pane_chrome::card_shadow;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, Kbd, icon};
use crate::tokens;

pub const CRASH_TITLE: &str = "Navo closed unexpectedly last time.";

type Press = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CrashKeys {
    pub reach: Option<String>,
    pub save: Option<String>,
    pub dismiss: Option<String>,
}

impl CrashKeys {
    #[must_use]
    pub fn of(keymap: &Keymap) -> Self {
        let first = |name: &str| keymap.keys_of(name).next().map(str::to_owned);
        Self {
            reach: first("crash_notice.focus"),
            save: first("crash_notice.save"),
            dismiss: first("crash_notice.dismiss"),
        }
    }
}

#[derive(IntoElement)]
pub struct CrashNotice {
    keys: CrashKeys,
    error: Option<SharedString>,
    is_focused: bool,
    is_saving: bool,
    on_save: Option<Press>,
    on_dismiss: Option<Press>,
}

impl CrashNotice {
    #[must_use]
    pub fn new(keys: CrashKeys) -> Self {
        Self {
            keys,
            error: None,
            is_focused: false,
            is_saving: false,
            on_save: None,
            on_dismiss: None,
        }
    }

    #[must_use]
    pub fn error(mut self, error: Option<SharedString>) -> Self {
        self.error = error;
        self
    }

    #[must_use]
    pub fn focused(mut self, is_focused: bool) -> Self {
        self.is_focused = is_focused;
        self
    }

    #[must_use]
    pub fn saving(mut self, is_saving: bool) -> Self {
        self.is_saving = is_saving;
        self
    }

    #[must_use]
    pub fn on_save(mut self, press: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_save = Some(Rc::new(press));
        self
    }

    #[must_use]
    pub fn on_dismiss(mut self, press: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(press));
        self
    }
}

fn action(t: &tokens::Theme, button: Button, (press, key): (Option<Press>, Option<Kbd>)) -> Div {
    let button = match press {
        Some(press) => button.on_click(move |_, window, cx| press(window, cx)),
        None => button,
    };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .child(button)
        .children(key)
}

fn shown_key(id: &'static str, key: Option<&String>, is_shown: bool) -> Option<Kbd> {
    key.filter(|_| is_shown).and_then(|key| Kbd::new(id, key))
}

fn text(t: &tokens::Theme, error: Option<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(t.spacing.xxs)
        .child(div().font_weight(FontWeight::MEDIUM).child(CRASH_TITLE))
        .when_some(error, |text, error| {
            text.child(
                div()
                    .id("crash-notice-error")
                    .role(Role::Status)
                    .aria_label(error.clone())
                    .test_support()
                    .text_size(t.typography.small)
                    .text_color(t.colors.mark)
                    .child(error),
            )
        })
}

impl CrashNotice {
    fn actions(self, t: &tokens::Theme) -> [Div; 2] {
        let keys = &self.keys;
        let save = Button::new("crash-notice-save", "Save report")
            .variant(ButtonVariant::Primary)
            .disabled(self.is_saving);
        let dismiss = Button::new("crash-notice-dismiss", "Dismiss").variant(ButtonVariant::Ghost);
        let save_key = shown_key("crash-notice-save-key", keys.save.as_ref(), self.is_focused);
        let dismiss_key = shown_key(
            "crash-notice-dismiss-key",
            keys.dismiss.as_ref(),
            self.is_focused,
        );
        [
            action(t, save, (self.on_save, save_key)),
            action(t, dismiss, (self.on_dismiss, dismiss_key)),
        ]
    }
}

impl RenderOnce for CrashNotice {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let is_focused = self.is_focused;
        let reach = shown_key(
            "crash-notice-reach-key",
            self.keys.reach.as_ref(),
            !is_focused,
        );
        let text = text(t, self.error.take());
        div()
            .id("crash-notice-card")
            .role(Role::Status)
            .aria_label(CRASH_TITLE)
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.lg)
            .px(t.spacing.lg)
            .py(t.spacing.md)
            .rounded(t.radii.card)
            .bg(t.surfaces.card)
            .shadow(card_shadow(t, is_focused))
            .child(icon(IconName::TriangleAlert, t.spacing.icon, t.colors.warn))
            .child(text)
            .children(reach)
            .children(self.actions(t))
    }
}
