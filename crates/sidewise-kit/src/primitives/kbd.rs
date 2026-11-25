use gpui_kit::base::TestSupportExt;
use gpui_kit::component::kbd::Kbd as KitKbd;
use gpui_kit::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, Keystroke, ParentElement,
    RenderOnce, Role, StatefulInteractiveElement, Styled, Window, div, transparent_black,
};

use crate::host_os::parse_keystroke;
use crate::tokens;

#[derive(IntoElement)]
pub struct Kbd {
    id: ElementId,
    keystroke: Keystroke,
    is_keycap: bool,
}

impl Kbd {
    pub fn new(id: impl Into<ElementId>, keys: &str) -> Option<Self> {
        let keystroke = parse_keystroke(keys).ok()?;
        Some(Self {
            id: id.into(),
            keystroke,
            is_keycap: false,
        })
    }

    pub fn keycap(mut self) -> Self {
        self.is_keycap = true;
        self
    }

    pub fn text(&self) -> String {
        KitKbd::format(&self.keystroke)
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::assets::load_mono_fonts(cx);
        let t = tokens::current(cx);
        let text = self.text();
        let key = KitKbd::new(self.keystroke)
            .outline()
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.key)
            .font_weight(FontWeight::MEDIUM);
        let key = if self.is_keycap {
            key.rounded(t.radii.keycap)
                .bg(t.surfaces.keycap_face)
                .border_color(t.surfaces.keycap_edge)
                .text_color(t.surfaces.keycap_label)
                .shadow(t.shadows.keycap.clone())
        } else {
            key.rounded(t.radii.sm)
                .bg(transparent_black())
                .border_color(t.colors.line)
                .text_color(t.colors.muted)
        };
        div()
            .id(self.id)
            .role(Role::Label)
            .aria_label(text)
            .test_support()
            .flex_none()
            .child(key)
    }
}
