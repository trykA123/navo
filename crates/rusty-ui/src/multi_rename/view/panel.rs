use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{BoxShadow, RenderOnce, Role, Size, deferred};
use rusty_l10n::{t, tr};

use super::fields::Fields;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconButton, IconName};
use crate::tokens::{self, Theme};

const MAX_WIDTH: f32 = 1120.;
const SIDE_GAP: f32 = 64.;
const HEIGHT_GAP: f32 = 96.;

type Pressed = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct RenamePanel {
    title: SharedString,
    fields: Fields,
    preview: AnyElement,
    tools: Option<AnyElement>,
    problems_only: bool,
    presets_menu: Option<AnyElement>,
    on_problems_only: Option<Pressed>,
    on_presets: Option<Pressed>,
}

impl RenamePanel {
    pub fn new(title: impl Into<SharedString>, fields: Fields, preview: impl IntoElement) -> Self {
        Self {
            title: title.into(),
            fields,
            preview: preview.into_any_element(),
            tools: None,
            problems_only: false,
            presets_menu: None,
            on_problems_only: None,
            on_presets: None,
        }
    }

    pub fn problems_only(mut self, is_on: bool) -> Self {
        self.problems_only = is_on;
        self
    }

    pub fn tools(mut self, tools: impl IntoElement) -> Self {
        self.tools = Some(tools.into_any_element());
        self
    }

    pub fn presets_menu(mut self, menu: impl IntoElement) -> Self {
        self.presets_menu = Some(menu.into_any_element());
        self
    }

    pub fn on_problems_only(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_problems_only = Some(Rc::new(handler));
        self
    }

    pub fn on_presets(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_presets = Some(Rc::new(handler));
        self
    }
}

#[must_use]
pub fn rename_title(count: usize, folder: &str) -> String {
    t!("rename-title", count = count, folder = folder.to_owned())
}

#[must_use]
pub fn scrim(t: &Theme) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(t.surfaces.scrim)
}

#[must_use]
pub fn panel_size(viewport: Size<Pixels>) -> Size<Pixels> {
    size(
        (viewport.width - px(SIDE_GAP))
            .min(px(MAX_WIDTH))
            .max(px(0.)),
        (viewport.height - px(HEIGHT_GAP)).max(px(0.)),
    )
}

fn pressed(handler: Option<Pressed>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        if let Some(handler) = &handler {
            handler(window, cx);
        }
    }
}

fn shadow(t: &Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: t.shadows.ink.opacity(0.24),
        offset: point(px(0.), px(12.)),
        blur_radius: px(32.),
        spread_radius: px(0.),
        inset: false,
    }]
}

impl RenamePanel {
    fn header(&mut self, t: &Theme) -> Div {
        let presets = Button::new("rename-presets", tr("rename-presets"))
            .variant(ButtonVariant::Ghost)
            .icon(IconName::ChevronDown)
            .on_click(pressed(self.on_presets.take()));
        let menu = self.presets_menu.take().map(|menu| {
            deferred(div().absolute().top_full().right_0().pt(px(4.)).child(menu)).with_priority(1)
        });
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .py(t.spacing.md)
            .border_b_1()
            .border_color(t.colors.line)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .child(div().relative().child(presets).children(menu))
    }

    fn filter_row(&mut self, t: &Theme) -> Div {
        let label = tr("rename-problems-only");
        let toggle = IconButton::new(
            "rename-problems-only",
            IconName::TriangleAlert,
            label.clone(),
        )
        .selected(self.problems_only)
        .on_click(pressed(self.on_problems_only.take()));
        let color = if self.problems_only {
            t.colors.text
        } else {
            t.colors.muted
        };
        div()
            .flex()
            .flex_none()
            .justify_end()
            .items_center()
            .gap(t.spacing.xs)
            .px(t.spacing.xl)
            .pb(t.spacing.xs)
            .text_size(t.typography.small)
            .text_color(color)
            .child(toggle)
            .child(label)
    }
}

impl RenderOnce for RenamePanel {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let area = panel_size(window.viewport_size());
        let header = self.header(&t);
        let filter = self.filter_row(&t);
        div()
            .id("multi-rename")
            .role(Role::Dialog)
            .aria_label(self.title.clone())
            .test_support()
            .flex()
            .flex_col()
            .w(area.width)
            .h(area.height)
            .rounded(t.radii.lg)
            .border_1()
            .border_color(t.colors.line)
            .bg(t.colors.surface)
            .shadow(shadow(&t))
            .overflow_hidden()
            .text_color(t.colors.text)
            .text_size(t.typography.body)
            .child(header)
            .child(
                div()
                    .flex_none()
                    .px(t.spacing.xl)
                    .py(t.spacing.lg)
                    .child(self.fields),
            )
            .child(filter)
            .children(self.tools)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(t.colors.line)
                    .child(self.preview),
            )
    }
}
