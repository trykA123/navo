mod menu;
mod tokens;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    App, Div, ElementId, Entity, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, transparent_black,
};
use rusty_core::rename::{ExtCase, NameCase, RenameRules};
use rusty_l10n::tr;

use crate::primitives::{
    Button, ButtonVariant, IconButton, IconName, TextField, TextFieldState, icon,
};
use crate::tokens::{self as theme, Theme};

pub use menu::{FieldMenu, MenuRow};
pub use tokens::{TOKENS, Token, matching};

type EventHandler = Rc<dyn Fn(FieldsEvent, &mut Window, &mut App)>;

const FIELD_MIN: f32 = 240.;
const NUMBER_WIDTH: f32 = 72.;
const CASE_WIDTH: f32 = 160.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mask {
    Name,
    Extension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Regex,
    MatchCase,
    FirstOnly,
    NameOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldsEvent {
    Tokens(Mask),
    Toggle(Toggle),
    NameCase,
    ExtensionCase,
}

#[derive(Clone)]
pub struct FieldInputs {
    pub name_mask: Entity<TextFieldState>,
    pub ext_mask: Entity<TextFieldState>,
    pub find: Entity<TextFieldState>,
    pub replace: Entity<TextFieldState>,
    pub start: Entity<TextFieldState>,
    pub step: Entity<TextFieldState>,
    pub digits: Entity<TextFieldState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldErrors {
    pub name_mask: Option<SharedString>,
    pub ext_mask: Option<SharedString>,
    pub find: Option<SharedString>,
    pub counter: Option<SharedString>,
}

#[must_use]
pub fn name_case_text(case: NameCase) -> String {
    tr(match case {
        NameCase::Unchanged => "rename-case-unchanged",
        NameCase::Lower => "rename-case-lower",
        NameCase::Upper => "rename-case-upper",
        NameCase::FirstLetter => "rename-case-first",
        NameCase::Title => "rename-case-title",
    })
}

#[must_use]
pub fn ext_case_text(case: ExtCase) -> String {
    tr(match case {
        ExtCase::Unchanged => "rename-case-unchanged",
        ExtCase::Lower => "rename-case-lower",
        ExtCase::Upper => "rename-case-upper",
    })
}

#[derive(IntoElement)]
pub struct Fields {
    id: ElementId,
    inputs: FieldInputs,
    rules: RenameRules,
    errors: FieldErrors,
    on_event: Option<EventHandler>,
}

impl Fields {
    pub fn new(id: impl Into<ElementId>, inputs: &FieldInputs, rules: &RenameRules) -> Self {
        Self {
            id: id.into(),
            inputs: inputs.clone(),
            rules: rules.clone(),
            errors: FieldErrors::default(),
            on_event: None,
        }
    }

    pub fn errors(mut self, errors: FieldErrors) -> Self {
        self.errors = errors;
        self
    }

    pub fn on_event(
        mut self,
        handler: impl Fn(FieldsEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}

struct CaseChoice {
    id: &'static str,
    label: String,
    value: String,
    event: FieldsEvent,
}

struct Cell<'a> {
    t: &'a Theme,
    label: String,
    input: &'a Entity<TextFieldState>,
    error: Option<&'a SharedString>,
}

fn cell(id: &'static str, cell: Cell<'_>) -> Div {
    let Cell {
        t,
        label,
        input,
        error,
    } = cell;
    let border = if error.is_some() {
        t.colors.mark
    } else {
        transparent_black()
    };
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .child(caption(t, label.clone()))
        .child(
            div()
                .rounded(t.radii.sm)
                .border_1()
                .border_color(border)
                .child(TextField::new(id, input, label)),
        )
}

fn caption(t: &Theme, text: String) -> Div {
    div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}

fn error_line(t: &Theme, id: &'static str, error: &SharedString) -> impl IntoElement + use<> {
    div()
        .id(id)
        .role(Role::Alert)
        .aria_label(error.clone())
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(icon(IconName::TriangleAlert, t.spacing.icon, t.colors.mark))
        .child(error.clone())
}

impl Fields {
    fn emit(
        &self,
        event: FieldsEvent,
    ) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
        let handler = self.on_event.clone();
        move |_, window, cx| {
            if let Some(handler) = &handler {
                handler(event, window, cx);
            }
        }
    }

    fn mask(&self, t: &Theme, mask: Mask) -> Div {
        let (id, label, input, error) = match mask {
            Mask::Name => (
                "name-mask",
                "rename-name-mask",
                &self.inputs.name_mask,
                &self.errors.name_mask,
            ),
            Mask::Extension => (
                "ext-mask",
                "rename-extension-mask",
                &self.inputs.ext_mask,
                &self.errors.ext_mask,
            ),
        };
        let (button_id, error_id) = match mask {
            Mask::Name => ("name-tokens", "name-mask-error"),
            Mask::Extension => ("ext-tokens", "ext-mask-error"),
        };
        let field = cell(
            id,
            Cell {
                t,
                label: tr(label),
                input,
                error: error.as_ref(),
            },
        );
        let insert = Button::new(button_id, "[\u{a0}]")
            .variant(ButtonVariant::Ghost)
            .tooltip(tr("rename-insert-token"))
            .on_click(self.emit(FieldsEvent::Tokens(mask)));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(FIELD_MIN))
            .gap(t.spacing.xs)
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(t.spacing.sm)
                    .child(field.flex_1().min_w_0())
                    .child(insert),
            )
            .children(error.as_ref().map(|error| error_line(t, error_id, error)))
    }

    fn toggles(&self, t: &Theme) -> Div {
        let find = &self.rules.find;
        let items = [
            (Toggle::Regex, IconName::Regex, "rename-regex", find.regex),
            (
                Toggle::MatchCase,
                IconName::CaseSensitive,
                "rename-match-case",
                find.match_case,
            ),
            (
                Toggle::FirstOnly,
                IconName::Replace,
                "rename-first-only",
                find.first_only,
            ),
            (
                Toggle::NameOnly,
                IconName::Type,
                "rename-name-only",
                find.name_only,
            ),
        ];
        let buttons =
            items
                .into_iter()
                .enumerate()
                .map(|(index, (toggle, glyph, label, is_on))| {
                    IconButton::new(("rename-toggle", index), glyph, tr(label))
                        .selected(is_on)
                        .on_click(self.emit(FieldsEvent::Toggle(toggle)))
                });
        div()
            .flex()
            .items_center()
            .gap(t.spacing.xs)
            .children(buttons)
    }

    fn find_row(&self, t: &Theme) -> Div {
        let find = cell(
            "find",
            Cell {
                t,
                label: tr("rename-find"),
                input: &self.inputs.find,
                error: self.errors.find.as_ref(),
            },
        );
        let replace = cell(
            "replace",
            Cell {
                t,
                label: tr("rename-replace"),
                input: &self.inputs.replace,
                error: None,
            },
        );
        div()
            .flex()
            .flex_wrap()
            .items_end()
            .gap(t.spacing.md)
            .child(div().flex_1().min_w(px(FIELD_MIN)).child(find))
            .child(div().flex_1().min_w(px(FIELD_MIN)).child(replace))
            .child(self.toggles(t))
    }

    fn case_button(&self, t: &Theme, choice: CaseChoice) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(t.spacing.xs)
            .w(px(CASE_WIDTH))
            .child(caption(t, choice.label))
            .child(
                Button::new(choice.id, choice.value)
                    .icon(IconName::ChevronDown)
                    .on_click(self.emit(choice.event)),
            )
    }

    fn counter(&self, t: &Theme) -> Div {
        let fields = [
            ("counter-start", "rename-counter-start", &self.inputs.start),
            ("counter-step", "rename-counter-step", &self.inputs.step),
            (
                "counter-digits",
                "rename-counter-digits",
                &self.inputs.digits,
            ),
        ];
        let error = self.errors.counter.as_ref();
        let cells = fields.into_iter().map(|(id, label, input)| {
            cell(
                id,
                Cell {
                    t,
                    label: tr(label),
                    input,
                    error,
                },
            )
            .w(px(NUMBER_WIDTH))
        });
        div().flex().gap(t.spacing.sm).children(cells)
    }

    fn case_row(&self, t: &Theme) -> Div {
        let rules = &self.rules;
        let name = self.case_button(
            t,
            CaseChoice {
                id: "name-case",
                label: tr("rename-name-case"),
                value: name_case_text(rules.name_case),
                event: FieldsEvent::NameCase,
            },
        );
        let extension = self.case_button(
            t,
            CaseChoice {
                id: "ext-case",
                label: tr("rename-extension-case"),
                value: ext_case_text(rules.ext_case),
                event: FieldsEvent::ExtensionCase,
            },
        );
        div()
            .flex()
            .flex_wrap()
            .items_end()
            .gap(t.spacing.md)
            .child(name)
            .child(extension)
            .child(self.counter(t))
    }
}

impl RenderOnce for Fields {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme::current(cx).clone();
        let counter_error = self.errors.counter.clone();
        let find_error = self.errors.find.clone();
        div()
            .id(self.id.clone())
            .flex()
            .flex_col()
            .gap(t.spacing.lg)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(t.spacing.md)
                    .child(self.mask(&t, Mask::Name))
                    .child(self.mask(&t, Mask::Extension)),
            )
            .child(self.find_row(&t))
            .children(find_error.map(|error| error_line(&t, "find-error", &error)))
            .child(self.case_row(&t))
            .children(counter_error.map(|error| error_line(&t, "counter-error", &error)))
    }
}
