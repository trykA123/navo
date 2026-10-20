use rusty_core::rename::{Counter, ExtCase, Find, NameCase, RenameRules};
use rusty_ui::multi_rename::view::fields::{
    FieldErrors, FieldInputs, FieldMenu, Fields, MenuRow, matching, name_case_text,
};
use rusty_ui::prelude::*;
use rusty_ui::primitives::{TextFieldState, text_field_state};
use rusty_ui::tokens::{self, Theme};

const NAME_CASES: [NameCase; 5] = [
    NameCase::Unchanged,
    NameCase::Lower,
    NameCase::Upper,
    NameCase::FirstLetter,
    NameCase::Title,
];

fn input(
    placeholder: &str,
    value: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TextFieldState> {
    let state = text_field_state(placeholder, window, cx);
    state.update(cx, |state, cx| state.set_value(value, window, cx));
    state
}

fn inputs(rules: &RenameRules, window: &mut Window, cx: &mut App) -> FieldInputs {
    let find = &rules.find;
    let counter = rules.counter;
    FieldInputs {
        name_mask: input("[N]", &rules.name_mask, window, cx),
        ext_mask: input("[E]", &rules.ext_mask, window, cx),
        find: input("", &find.text, window, cx),
        replace: input("", &find.replace, window, cx),
        start: input("1", &counter.start.to_string(), window, cx),
        step: input("1", &counter.step.to_string(), window, cx),
        digits: input("1", &counter.digits.to_string(), window, cx),
    }
}

fn filled() -> RenameRules {
    RenameRules {
        name_mask: "[N]_[C10+5:3]".into(),
        find: Find {
            text: "IMG_".into(),
            replace: "Holiday ".into(),
            match_case: true,
            name_only: true,
            ..Find::default()
        },
        name_case: NameCase::Title,
        ext_case: ExtCase::Lower,
        counter: Counter {
            start: 10,
            step: 5,
            digits: 3,
        },
        ..RenameRules::default()
    }
}

fn broken() -> RenameRules {
    RenameRules {
        name_mask: "[N]_[Q]".into(),
        ext_mask: "[E".into(),
        find: Find {
            text: "(IMG".into(),
            regex: true,
            ..Find::default()
        },
        counter: Counter {
            start: 1,
            step: 0,
            digits: 12,
        },
        ..RenameRules::default()
    }
}

fn errors() -> FieldErrors {
    FieldErrors {
        name_mask: Some("Unknown token [Q] at position 5".into()),
        ext_mask: Some("Unknown token [E at position 1".into()),
        find: Some("The regular expression is not valid".into()),
        counter: Some("Digits must be from 1 to 10 and Step at least 1".into()),
    }
}

pub struct MultiRenamePage {
    filled: (RenameRules, FieldInputs),
    broken: (RenameRules, FieldInputs),
    filter: Entity<TextFieldState>,
}

impl MultiRenamePage {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let (filled, broken) = (filled(), broken());
        Self {
            filled: (filled.clone(), inputs(&filled, window, cx)),
            broken: (broken.clone(), inputs(&broken, window, cx)),
            filter: input("Type to filter tokens", "name", window, cx),
        }
    }
}

fn panel(t: &Theme, title: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.md)
        .p(t.spacing.lg)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.colors.background)
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(title.to_owned()),
        )
}

impl Render for MultiRenamePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let tokens = matching("name").iter().map(MenuRow::from_token).collect();
        let cases = NAME_CASES.map(|case| MenuRow {
            text: name_case_text(case).into(),
            is_current: case == NameCase::Title,
        });
        let side = |inner: Div| inner.flex_1().min_w(px(420.));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .gap(t.spacing.lg)
            .p(t.spacing.lg)
            .bg(t.colors.surface)
            .child(
                div()
                    .flex()
                    .gap(t.spacing.lg)
                    .child(side(panel(&t, "Filled in, toggles on").child(Fields::new(
                        "fields-filled",
                        &self.filled.1,
                        &self.filled.0,
                    ))))
                    .child(side(
                        panel(&t, "Every field with an error").child(
                            Fields::new("fields-broken", &self.broken.1, &self.broken.0)
                                .errors(errors()),
                        ),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(t.spacing.lg)
                    .child(
                        panel(&t, "Token menu, filtered")
                            .child(FieldMenu::new("token-menu", tokens).filter(&self.filter)),
                    )
                    .child(
                        panel(&t, "Token menu, no match")
                            .child(FieldMenu::new("token-menu-empty", Vec::new())),
                    )
                    .child(
                        panel(&t, "Case choices")
                            .child(FieldMenu::new("case-menu", cases.into()).selected(3)),
                    ),
            )
    }
}
