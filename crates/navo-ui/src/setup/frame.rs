//! The setup window's frame: the step rail on the left, the page, and the button footer below it.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, Entity, FocusHandle, FontWeight, InteractiveElement, RenderOnce, Role,
    StatefulInteractiveElement,
};

use super::keys::{FooterButton, StepMark, footer, steps};
use super::{OnAction, Page, SetupPage, SetupView, Strings};
use crate::primitives::access_label::plain_text;
use crate::primitives::{Button, ButtonVariant, TextFieldState, icon};
use crate::tokens::{self, Theme};
use crate::{AnyElement, App, Div, IntoElement, ParentElement, Styled, Window, div, px};

const RAIL_WIDTH: f32 = 178.;
const MARK_SIZE: f32 = 16.;
const SCENE_WIDTH: f32 = 84.;
const SCENE_HEIGHT: f32 = 36.;
const BAR_WIDTH: f32 = 16.;
const BAR_HEIGHT: f32 = 4.;
/// One sparring round of the n and the _ while Setup works (owner-approved exception, W9).
const SPAR: Duration = Duration::from_millis(2400);

/// Draws the whole window: rail, page and footer. `field` is the editable Folder field and
/// `primary` the focus the footer's primary button tracks.
pub fn render_frame(
    view: &SetupView,
    strings: &Strings,
    on_action: OnAction,
    (field, primary): (Option<Entity<TextFieldState>>, Option<FocusHandle>),
) -> AnyElement {
    Frame {
        primary,
        page: Page {
            view: view.clone(),
            strings: strings.clone(),
            on_action,
            field,
        },
    }
    .into_any_element()
}

/// The frame around one page; it reads the page's view, texts and action sink.
#[derive(IntoElement)]
struct Frame {
    page: Page,
    primary: Option<FocusHandle>,
}

impl RenderOnce for Frame {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let spar = !cx.reduce_motion() && self.page.view.page == SetupPage::Installing;
        let rail = rail(t, &self.page.view, &self.page.strings, spar);
        let buttons = footer_row(t, &self.page, self.primary.as_ref());
        let body = div()
            .flex_1()
            .min_h_0()
            .px(t.spacing.xl)
            .pt(t.spacing.xl)
            .child(self.page);
        div()
            .size_full()
            .flex()
            .bg(t.colors.surface)
            .text_color(t.colors.text)
            .child(rail)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(body)
                    .child(buttons),
            )
    }
}

/// Builds the rail: the brand, the six steps and the n/_ scene at the bottom.
fn rail(t: &Theme, view: &SetupView, strings: &Strings, spar: bool) -> Div {
    let version = strings.rail_version.replace("{version}", &view.version);
    let brand = div()
        .flex()
        .flex_col()
        .child(
            div()
                .font_family(t.typography.brand_font.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(t.typography.body)
                .child(strings.title.clone()),
        )
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(version),
        );
    let labels = [
        strings.step_welcome.to_string(),
        strings.licence_title.to_string(),
        strings.folder_title.to_string(),
        strings.shortcuts_title.to_string(),
        strings.installing_bar.to_string(),
        plain_text(&strings.finish),
    ];
    let rows = labels
        .into_iter()
        .zip(steps(view))
        .enumerate()
        .map(|(index, (label, mark))| step_row(t, index, label, mark));
    div()
        .w(px(RAIL_WIDTH))
        .flex_none()
        .flex()
        .flex_col()
        .gap(t.spacing.xl)
        .p(t.spacing.lg)
        .bg(t.colors.rail)
        .border_r_1()
        .border_color(t.colors.line)
        .child(brand)
        .child(div().flex().flex_col().gap(t.spacing.xxs).children(rows))
        .child(div().mt_auto().child(scene(t, spar)))
}

/// Builds one step row; the current step is highlighted and named for screen readers.
fn step_row(t: &Theme, index: usize, label: String, mark: StepMark) -> impl IntoElement {
    let current = matches!(mark, StepMark::Current | StepMark::Failed);
    let marker = match mark {
        StepMark::Done => {
            icon(IconName::Check, px(MARK_SIZE), t.colors.accent_text).into_any_element()
        }
        StepMark::Skipped => {
            icon(IconName::Minus, px(MARK_SIZE), t.colors.muted).into_any_element()
        }
        StepMark::Failed => icon(IconName::X, px(MARK_SIZE), t.colors.mark).into_any_element(),
        StepMark::Current => dot(t.colors.accent, t.spacing.sm).into_any_element(),
        StepMark::Todo => dot(t.colors.line, t.spacing.xs).into_any_element(),
    };
    let text = if matches!(mark, StepMark::Todo | StepMark::Skipped) {
        t.colors.muted
    } else {
        t.colors.text
    };
    div()
        .id(("setup-step", index))
        .role(Role::ListItem)
        .aria_label(label.clone())
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .px(t.spacing.sm)
        .py(t.spacing.xs)
        .rounded(t.radii.md)
        .text_size(t.typography.small)
        .text_color(text)
        .when(current, |row| {
            row.bg(t.colors.selection).font_weight(FontWeight::SEMIBOLD)
        })
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(MARK_SIZE))
                .child(marker),
        )
        .child(label)
}

/// Builds a round dot of `size` in `color`.
fn dot(color: gpui_kit::Hsla, size: gpui_kit::Pixels) -> Div {
    div().size(size).rounded_full().bg(color)
}

/// Draws the brand's n and _ in the rail foot; while Installing they spar (off under reduced motion).
fn scene(t: &Theme, spar: bool) -> AnyElement {
    let n = div()
        .absolute()
        .left(px(10.))
        .bottom(px(3.))
        .font_family(t.typography.brand_font.clone())
        .font_weight(FontWeight::BOLD)
        .text_size(px(28.))
        .line_height(px(28.))
        .child("n");
    let bar = div()
        .absolute()
        .left(px(SCENE_WIDTH - 10. - BAR_WIDTH))
        .bottom(px(6.))
        .w(px(BAR_WIDTH))
        .h(px(BAR_HEIGHT))
        .rounded(px(1.))
        .bg(t.colors.accent);
    let stage = div()
        .id("setup-scene")
        .relative()
        .w(px(SCENE_WIDTH))
        .h(px(SCENE_HEIGHT));
    if !spar {
        return stage.child(n).child(bar).into_any_element();
    }
    let round = Animation::new(SPAR).repeat();
    let n = n.with_animation("spar-n", round.clone(), |n, delta| {
        n.left(px(10. + offset(&N_STEPS, delta)))
    });
    let bar = bar.with_animation("spar-bar", round, |bar, delta| {
        bar.left(px(SCENE_WIDTH - 10. - BAR_WIDTH + offset(&BAR_STEPS, delta)))
    });
    stage.child(n).child(bar).into_any_element()
}

/// Sideways offsets (progress 0..1, px) of the n, from the prototype's spar keyframes.
const N_STEPS: [(f32, f32); 9] = [
    (0., 0.),
    (0.22, 0.),
    (0.32, 14.),
    (0.38, 9.),
    (0.48, 0.),
    (0.62, 0.),
    (0.70, -4.),
    (0.78, -10.),
    (0.90, 0.),
];
/// Sideways offsets (progress 0..1, px) of the _.
const BAR_STEPS: [(f32, f32); 9] = [
    (0., 0.),
    (0.22, 0.),
    (0.30, -30.),
    (0.36, -20.),
    (0.46, 0.),
    (0.58, 0.),
    (0.68, -34.),
    (0.74, -30.),
    (0.88, 0.),
];

/// Interpolates `steps` linearly at `progress` (0..1); past the last step the offset is its value.
fn offset(steps: &[(f32, f32)], progress: f32) -> f32 {
    let mut last = (0., 0.);
    for &(at, value) in steps {
        if progress <= at {
            let span = (at - last.0).max(f32::EPSILON);
            return last.1 + (value - last.1) * (progress - last.0) / span;
        }
        last = (at, value);
    }
    last.1
}

/// Builds the footer: Back, Close and the primary button, right-aligned under the page.
fn footer_row(t: &Theme, page: &Page, primary: Option<&FocusHandle>) -> Div {
    let buttons = footer(&page.view, &page.strings)
        .into_iter()
        .map(|button| footer_button(button, page, primary));
    div()
        .flex()
        .flex_none()
        .justify_end()
        .items_center()
        .gap(t.spacing.sm)
        .px(t.spacing.lg)
        .py(t.spacing.md)
        .border_t_1()
        .border_color(t.colors.line)
        .children(buttons)
}

/// Builds one footer button whose click sends its action.
fn footer_button(button: FooterButton, page: &Page, focus: Option<&FocusHandle>) -> Button {
    let id = match button.action {
        super::SetupAction::Back => "setup-back",
        _ if button.primary => "setup-primary",
        _ => "setup-close",
    };
    let send = page.on_action.clone();
    let action = button.action;
    let variant = if button.primary {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Secondary
    };
    let focus = focus.filter(|_| button.primary);
    Button::new(id, button.label)
        .when_some(focus, Button::track_focus)
        .access_key(page.view.show_keys)
        .variant(variant)
        .disabled(!button.enabled)
        .on_click(move |_, window, cx| send(action, window, cx))
}
