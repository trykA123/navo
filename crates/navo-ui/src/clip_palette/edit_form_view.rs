//! Drawing of the saved item form: a header, the fields for the chosen kind and a footer.

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::{Editor, Input};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use navo_l10n::tr;

use super::chrome::{footer_bar, footer_button, hint};
use super::edit_form::{Field, Form, FormKind, Problem};
use super::menu::fade_in;
use super::password::Password;
use super::rows::{MASK, RowElement};
use super::view::ClipPalette;
use crate::commands::active;
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

const HEADER: f32 = 44.;
const FIELD: f32 = 32.;
const BODY_MIN: f32 = 72.;
const COLUMN_MIN: f32 = 220.;
const CARET: f32 = 1.;

/// The first key of a clip palette command that a key chip can draw.
fn key_of(command: &str, cx: &App) -> Option<String> {
    let mut keys = active(cx).keys_in(command, "ClipPalette");
    keys.find(|key| Kbd::new("probe", key).is_some())
        .map(str::to_owned)
}

pub(super) fn label(t: &Theme, text: String) -> Div {
    div()
        .text_size(t.typography.small)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.muted)
        .child(text)
}

fn problem_line(t: &Theme, text: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .child(icon(
            IconName::TriangleAlert,
            t.typography.small,
            t.colors.warn,
        ))
        .child(div().min_w_0().child(text))
}

/// A bordered box for a field; the border turns to the accent while the field has focus.
fn framed(t: &Theme, id: impl Into<ElementId>, is_focused: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .min_w_0()
        .rounded(t.radii.md)
        .border_1()
        .border_color(if is_focused {
            t.colors.accent
        } else {
            t.colors.line
        })
        .bg(t.surfaces.field)
}

/// What sits in the password box: dots or the text, the caret, and the hint while empty.
fn password_content(
    t: &Theme,
    password: &Password,
    (is_focused, hint_text): (bool, Option<String>),
) -> Div {
    let shown = if password.is_shown() {
        SharedString::from(password.text().to_owned())
    } else {
        let count = password.text().chars().count();
        SharedString::from(MASK.chars().cycle().take(count).collect::<String>())
    };
    div()
        .flex()
        .flex_1()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .font_family(t.typography.mono_font.clone())
        .child(shown)
        .when(is_focused, |row| {
            let caret = div().flex_none().w(px(CARET)).h(t.typography.body);
            row.child(caret.bg(t.colors.accent))
        })
        .when(password.is_empty() && !is_focused, |row| {
            let hint = hint_text.clone().unwrap_or_default();
            let font = t.typography.ui_font.clone();
            row.child(
                div()
                    .font_family(font)
                    .text_color(t.colors.faint)
                    .child(hint),
            )
        })
}

impl ClipPalette {
    /// The inline message under `field`, faded in.
    fn problem_under(
        &self,
        t: &Theme,
        (form, field): (&Form, Field),
        cx: &App,
    ) -> Option<AnyElement> {
        let problem = form.problem.as_ref().filter(|p| p.field() == Some(field))?;
        let line = problem_line(t, problem.text())
            .id("clip-form-problem")
            .role(Role::Status)
            .aria_label(SharedString::from(problem.text()))
            .test_support();
        Some(fade_in(line, "clip-form-problem-in", t, cx))
    }

    fn text_field(
        &self,
        t: &Theme,
        (id, text, field): (&'static str, String, Field),
        (form, window, cx): (&Form, &Window, &App),
    ) -> Div {
        let state = if field == Field::User {
            &form.user
        } else {
            &form.title
        };
        let is_focused = form.handle(field, cx).is_focused(window);
        let input = Input::new(state)
            .id(id)
            .aria_label(SharedString::from(text.clone()))
            .appearance(false)
            .h(px(FIELD - 2.))
            .text_size(t.typography.body);
        div()
            .flex()
            .flex_col()
            .gap(t.spacing.xs)
            .child(label(t, text))
            .child(
                framed(t, (id, 1usize), is_focused)
                    .test_support()
                    .child(div().flex_1().child(input)),
            )
            .children(self.problem_under(t, (form, field), cx))
    }

    fn password_field(
        &self,
        t: &Theme,
        (form, window): (&Form, &Window),
        cx: &mut Context<Self>,
    ) -> Div {
        let password = &form.password;
        let is_focused = password.focus().is_focused(window);
        let text = tr("clip-form-password");
        let hint_text = form.keeps_password().then(|| tr("clip-form-password-keep"));
        let content = password_content(t, password, (is_focused, hint_text));
        let label_key = if password.is_shown() {
            "clip-act-hide"
        } else {
            "clip-act-show"
        };
        let toggle = hint(
            t,
            "clip-form-reveal-key",
            key_of("clip_palette.toggle_reveal", cx).as_deref(),
            tr(label_key),
        )
        .id("clip-form-reveal")
        .role(Role::Button)
        .cursor_pointer()
        .on_click(cx.listener(|this, _, _, cx| this.form_reveal(cx)));
        let field = framed(t, "clip-form-password", is_focused)
            .role(Role::Group)
            .aria_label(SharedString::from(text.clone()))
            .test_support()
            .h(px(FIELD))
            .px(t.spacing.md)
            .track_focus(password.focus())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.password_key(event, cx);
            }))
            .text_size(t.typography.body)
            .child(content)
            .child(toggle);
        div()
            .flex()
            .flex_col()
            .gap(t.spacing.xs)
            .child(label(t, text))
            .child(field)
            .children(self.problem_under(t, (form, Field::Password), cx))
    }

    fn body_field(&self, t: &Theme, (form, window): (&Form, &Window), cx: &App) -> Div {
        let is_focused = form.handle(Field::Body, cx).is_focused(window);
        let text = tr(if form.kind == FormKind::Path {
            "clip-form-path"
        } else {
            "clip-form-text"
        });
        let editor = Editor::new(&form.body)
            .h_full()
            .appearance(false)
            .bordered(false)
            .aria_label(SharedString::from(text.clone()))
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.small);
        let frame = framed(t, "clip-form-body", is_focused)
            .test_support()
            .flex_1()
            .items_start()
            .overflow_hidden()
            .child(div().size_full().child(editor));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(BODY_MIN))
            .gap(t.spacing.xs)
            .child(label(t, text))
            .child(frame)
            .children(self.problem_under(t, (form, Field::Body), cx))
    }

    fn fields(
        &self,
        t: &Theme,
        (form, window): (&Form, &Window),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let column = div()
            .id("clip-form")
            .role(Role::Group)
            .aria_label(SharedString::from(self.form_heading(form)))
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .py(t.spacing.md)
            .overflow_hidden()
            .child(self.choices(t, (form, window), cx))
            .child(self.text_field(
                t,
                ("clip-form-title", tr("clip-form-title"), Field::Title),
                (form, window, cx),
            ));
        if form.kind == FormKind::Secret {
            let user = self.text_field(
                t,
                ("clip-form-user", tr("clip-form-user"), Field::User),
                (form, window, cx),
            );
            let half = |field: Div| div().flex_1().min_w(px(COLUMN_MIN)).child(field);
            let secret = div()
                .flex()
                .flex_wrap()
                .gap_x(t.spacing.xl)
                .gap_y(t.spacing.md)
                .child(half(user))
                .child(half(self.password_field(t, (form, window), cx)));
            column.child(secret)
        } else {
            column.child(self.body_field(t, (form, window), cx))
        }
    }

    fn form_heading(&self, form: &Form) -> String {
        tr(if form.is_new() {
            "clip-form-new"
        } else {
            "clip-form-edit"
        })
    }

    fn header(&self, t: &Theme, form: &Form) -> RowElement {
        let glyph = if form.is_new() {
            IconName::Plus
        } else {
            IconName::FilePen
        };
        div()
            .id("clip-form-header")
            .role(Role::Heading)
            .aria_label(SharedString::from(self.form_heading(form)))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .h(px(HEADER))
            .px(t.spacing.xl)
            .border_b_1()
            .border_color(t.colors.line)
            .child(icon(glyph, t.spacing.icon, t.colors.muted))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.form_heading(form)),
            )
    }

    /// Save and Cancel, or the question that replaces them when Esc found a change.
    fn form_footer(&self, t: &Theme, form: &Form, cx: &mut Context<Self>) -> RowElement {
        if form.is_asking {
            let yes = footer_button(t, "clip-form-discard", tr("clip-form-discard-yes"), "enter")
                .on_click(cx.listener(|this, _, window, cx| this.close_form(window, cx)));
            let no = footer_button(t, "clip-form-keep", tr("clip-form-discard-no"), "escape")
                .on_click(cx.listener(|this, _, window, cx| this.form_back(window, cx)));
            let ask = div()
                .id("clip-form-ask")
                .role(Role::Status)
                .aria_label(SharedString::from(tr("clip-form-discard")))
                .test_support()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text)
                .child(tr("clip-form-discard"));
            return footer_bar(
                t,
                div().child(fade_in(ask, "clip-form-ask-in", t, cx)),
                vec![yes.into_any_element(), no.into_any_element()],
            );
        }
        let refusal = match &form.problem {
            Some(Problem::Refused(message)) => Some(problem_line(t, message.clone())),
            _ => None,
        };
        let save = footer_button(
            t,
            "clip-form-save",
            tr("clip-form-save"),
            key_of("clip_palette.save_form", cx).as_deref(),
        )
        .on_click(cx.listener(|this, _, window, cx| this.save_form(window, cx)));
        let cancel = footer_button(t, "clip-form-cancel", tr("clip-form-cancel"), "escape")
            .on_click(cx.listener(|this, _, window, cx| this.form_back(window, cx)));
        footer_bar(
            t,
            div().children(refusal),
            vec![save.into_any_element(), cancel.into_any_element()],
        )
    }

    /// The bands of the form, which replace the search row, tabs, list and footer.
    pub(super) fn form_view(
        &self,
        t: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(form) = self.form.as_ref() else {
            return Vec::new();
        };
        vec![
            self.header(t, form).into_any_element(),
            self.fields(t, (form, window), cx).into_any_element(),
            self.form_footer(t, form, cx).into_any_element(),
        ]
    }
}
