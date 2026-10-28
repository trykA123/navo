use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::{Button as KitButton, ButtonVariants};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ClickEvent, Hsla, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div,
};
use rusty_l10n::t;

use crate::primitives::{IconName, icon};
use crate::tokens;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSummary {
    pub name: SharedString,
    pub n: usize,
    pub total: usize,
    pub draft: u16,
}

#[derive(IntoElement)]
pub struct WorkspaceButton {
    summary: WorkspaceSummary,
    is_pinned: bool,
    on_click: Option<ClickHandler>,
}

impl WorkspaceButton {
    pub fn new(summary: WorkspaceSummary) -> Self {
        Self {
            summary,
            is_pinned: false,
            on_click: None,
        }
    }

    #[must_use]
    pub fn pinned(mut self, is_pinned: bool) -> Self {
        self.is_pinned = is_pinned;
        self
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

fn part(id: &'static str, text: String, color: Hsla) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .flex_none()
        .text_color(color)
        .child(text)
}

impl RenderOnce for WorkspaceButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let c = &t.colors;
        let s = self.summary;
        let count = t!("workspace-button-count", n = s.n, total = s.total);
        let label = t!(
            "workspace-button",
            name = s.name.to_string(),
            n = s.n,
            total = s.total
        );
        let content = div()
            .flex()
            .items_center()
            .gap(t.spacing.sm)
            .child(
                div()
                    .id("workspace-button-icon")
                    .test_support()
                    .flex()
                    .flex_none()
                    .child(icon(IconName::LayoutGrid, t.spacing.icon, c.muted)),
            )
            .child(part("workspace-button-name", s.name.to_string(), c.text))
            .child(part("workspace-button-count", count, c.muted))
            .when(s.draft > 0, |row| {
                row.child(part(
                    "workspace-button-draft",
                    t!("workspace-draft", count = s.draft),
                    c.warn,
                ))
            });
        KitButton::new("workspace-button")
            .ghost()
            .accessibility_label(label)
            .toggled(self.is_pinned)
            .selected(self.is_pinned)
            .tab_stop(false)
            .flex_none()
            .px(t.spacing.sm)
            .rounded(t.radii.sm)
            .text_size(t.typography.small)
            .when_some(self.on_click, |button, handler| {
                button.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .child(content)
    }
}
