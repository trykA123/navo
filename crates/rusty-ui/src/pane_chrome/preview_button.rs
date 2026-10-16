use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Role, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::file_list::row::inset_ring;
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens;

const KEYS: &str = "ctrl-q";
const GLYPH: f32 = 14.;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PreviewButton {
    id: ElementId,
    is_open: bool,
    on_click: Option<ClickHandler>,
}

impl PreviewButton {
    pub fn new(id: impl Into<ElementId>, is_open: bool) -> Self {
        Self {
            id: id.into(),
            is_open,
            on_click: None,
        }
    }

    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for PreviewButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let c = &t.colors;
        let (face, ring, label) = if self.is_open {
            (c.selection, c.accent, c.accent_text)
        } else {
            (gpui_kit::transparent_black(), t.surfaces.card_edge, c.text)
        };
        let hover = c.hover;
        let kbd = Kbd::new("preview-key", KEYS);
        let spoken = kbd.as_ref().map(Kbd::text);
        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label("Preview")
            .aria_selected(self.is_open)
            .when_some(spoken, |button, keys| button.aria_keyshortcuts(keys))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .h(t.spacing.tab_height)
            .pl(t.spacing.md)
            .pr(t.spacing.xs)
            .rounded(t.radii.md)
            .cursor_pointer()
            .bg(face)
            .text_color(label)
            .shadow(inset_ring(ring))
            .when(!self.is_open, |button| button.hover(move |s| s.bg(hover)))
            .when_some(self.on_click, |button, handler| {
                button.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .child(icon(IconName::PanelRight, px(GLYPH), label))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Preview"))
            .children(kbd)
    }
}
