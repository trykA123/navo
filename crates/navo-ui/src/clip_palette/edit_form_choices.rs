//! The kind and scope chips of the saved item form: two groups, each one Tab stop that Left and
//! Right walk.

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Context, Div, ElementId, FocusHandle, FontWeight, InteractiveElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use navo_l10n::tr;

use super::edit_form::{Form, FormKind};
use super::edit_form_view::label;
use super::rows::RowElement;
use super::view::ClipPalette;
use crate::tokens::Theme;

const CHIP: f32 = 24.;

/// A labelled group of chips that takes the focus as one stop; Left and Right pick.
fn chips(
    t: &Theme,
    (id, text): (&'static str, String),
    (handle, is_focused): (&FocusHandle, bool),
    chips: Vec<RowElement>,
) -> Div {
    let group = div()
        .id(id)
        .role(Role::TabList)
        .aria_label(SharedString::from(text.clone()))
        .test_support()
        .track_focus(handle)
        .flex()
        .gap(t.spacing.xs)
        .p(t.spacing.xxs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(if is_focused {
            t.colors.accent
        } else {
            t.colors.line
        })
        .children(chips);
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .child(label(t, text))
        .child(group)
}

impl ClipPalette {
    fn chip(&self, t: &Theme, (id, text): (ElementId, String), is_current: bool) -> RowElement {
        let theme = t.selected_content(is_current);
        let t = &*theme;
        div()
            .id(id)
            .role(Role::Tab)
            .aria_label(SharedString::from(text.clone()))
            .aria_selected(is_current)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .h(px(CHIP))
            .px(t.spacing.md)
            .rounded(t.radii.md)
            .border_1()
            .cursor_pointer()
            .text_size(t.typography.small)
            .border_color(t.colors.line)
            .when(is_current, |chip| {
                chip.bg(t.colors.selection)
                    .text_color(t.colors.on_selection)
                    .font_weight(FontWeight::SEMIBOLD)
            })
            .when(!is_current, |chip| {
                chip.text_color(t.colors.muted)
                    .hover(|chip| chip.bg(t.colors.hover))
            })
            .child(text)
    }

    pub(super) fn choices(
        &self,
        t: &Theme,
        (form, window): (&Form, &Window),
        cx: &mut Context<Self>,
    ) -> Div {
        let kinds = FormKind::ALL.into_iter().enumerate().map(|(at, kind)| {
            let is_current = kind == form.kind;
            let chip = self.chip(t, (("clip-form-kind", at).into(), kind.label()), is_current);
            chip.on_click(cx.listener(move |this, _, window, cx| {
                this.choose_kind(kind, window, cx);
            }))
        });
        let scopes = form.scopes.iter().enumerate().map(|(at, scope)| {
            let chip = self.chip(
                t,
                (("clip-form-scope", at).into(), self.scope_label(scope)),
                at == form.scope,
            );
            chip.on_click(cx.listener(move |this, _, window, cx| {
                this.choose_scope(at, window, cx);
            }))
        });
        let focused = |handle: &FocusHandle| handle.is_focused(window);
        let kind = chips(
            t,
            ("clip-form-kinds", tr("clip-form-kind")),
            (&form.kind_focus, focused(&form.kind_focus)),
            kinds.collect(),
        );
        let scope = (form.scopes.len() > 1).then(|| {
            chips(
                t,
                ("clip-form-scopes", tr("clip-form-scope")),
                (&form.scope_focus, focused(&form.scope_focus)),
                scopes.collect(),
            )
        });
        div()
            .flex()
            .flex_wrap()
            .gap_x(t.spacing.xl)
            .gap_y(t.spacing.md)
            .child(kind)
            .children(scope)
    }

    pub(super) fn choose_kind(
        &mut self,
        kind: FormKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.form.as_mut() {
            form.set_kind(kind);
            window.focus(&form.kind_focus, cx);
            cx.notify();
        }
    }

    pub(super) fn choose_scope(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut().filter(|form| at < form.scopes.len()) {
            form.scope = at;
            window.focus(&form.scope_focus, cx);
            cx.notify();
        }
    }
}
