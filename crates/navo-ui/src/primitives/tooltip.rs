use gpui_kit::base::TestSupportExt;
use gpui_kit::component::kbd::Kbd as KitKbd;
use gpui_kit::component::tooltip::Tooltip as KitTooltip;
use gpui_kit::{
    AnyView, App, InteractiveElement, Keystroke, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, div,
};

use crate::tokens;

pub struct Tooltip {
    text: SharedString,
    keystroke: Option<Keystroke>,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            keystroke: None,
        }
    }

    pub fn keys(mut self, keys: &str) -> Self {
        self.keystroke = Keystroke::parse(keys).ok();
        self
    }

    pub fn build(self, window: &mut Window, cx: &mut App) -> AnyView {
        let text = self.text;
        let key = self.keystroke.map(KitKbd::new);
        KitTooltip::element(move |_, cx| {
            let t = tokens::current(cx);
            div()
                .id("tooltip")
                .role(Role::Tooltip)
                .aria_label(text.clone())
                .test_support()
                .font_family(t.typography.ui_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.text)
                .child(text.clone())
        })
        .key_binding(key)
        .build(window, cx)
    }
}
