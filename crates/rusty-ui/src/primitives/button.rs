use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button as KitButton, ButtonVariants};
use gpui_kit::component::{Disableable, Icon, Selectable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{App, ClickEvent, ElementId, IntoElement, RenderOnce, SharedString, Styled, Window};

use crate::tokens;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ButtonVariant {
    #[default]
    Secondary,
    Primary,
    Ghost,
    Danger,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    is_disabled: bool,
    tooltip: Option<SharedString>,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::default(),
            is_disabled: false,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn disabled(mut self, is_disabled: bool) -> Self {
        self.is_disabled = is_disabled;
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let button = KitButton::new(self.id)
            .label(self.label)
            .small()
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.small)
            .disabled(self.is_disabled);
        let button = match self.variant {
            ButtonVariant::Primary => button.primary(),
            ButtonVariant::Secondary => button.outline(),
            ButtonVariant::Ghost => button.ghost(),
            ButtonVariant::Danger => button.danger(),
        };
        button
            .when_some(self.tooltip, KitButton::tooltip)
            .when_some(self.on_click, |button, handler| {
                button.on_click(move |event, window, cx| handler(event, window, cx))
            })
    }
}

#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: IconName,
    label: SharedString,
    is_selected: bool,
    is_disabled: bool,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            is_selected: false,
            is_disabled: false,
            on_click: None,
        }
    }

    pub fn selected(mut self, is_selected: bool) -> Self {
        self.is_selected = is_selected;
        self
    }

    pub fn disabled(mut self, is_disabled: bool) -> Self {
        self.is_disabled = is_disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let color = match (self.is_disabled, self.is_selected) {
            (true, _) => t.colors.faint,
            (false, true) => t.colors.text,
            (false, false) => t.colors.muted,
        };
        let glyph = Icon::new(self.icon).size(t.spacing.icon);
        KitButton::new(self.id)
            .icon(glyph)
            .ghost()
            .with_size(t.spacing.icon + t.spacing.sm * 2.)
            .text_color(color)
            .selected(self.is_selected)
            .disabled(self.is_disabled)
            .accessibility_label(self.label.clone())
            .tooltip(self.label)
            .when_some(self.on_click, |button, handler| {
                button.on_click(move |event, window, cx| handler(event, window, cx))
            })
    }
}
