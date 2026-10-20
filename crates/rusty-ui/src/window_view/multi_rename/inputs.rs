use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Subscription};
use rusty_core::rename::{Counter, Find, RenameRules};

use super::FerriteWindow;
use crate::multi_rename::view::fields::{FieldInputs, Toggle};
use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};

pub(super) fn create(rules: &RenameRules, window: &mut Window, cx: &mut App) -> FieldInputs {
    let mut field = |placeholder: &str| text_field_state(placeholder.to_owned(), window, cx);
    let inputs = FieldInputs {
        name_mask: field("[N]"),
        ext_mask: field("[E]"),
        find: field(""),
        replace: field(""),
        start: field("1"),
        step: field("1"),
        digits: field("1"),
    };
    fill(&inputs, rules, window, cx);
    inputs
}

fn all(inputs: &FieldInputs) -> [&Entity<TextFieldState>; 7] {
    [
        &inputs.name_mask,
        &inputs.ext_mask,
        &inputs.find,
        &inputs.replace,
        &inputs.start,
        &inputs.step,
        &inputs.digits,
    ]
}

pub(super) fn stops(inputs: &FieldInputs, cx: &App) -> Vec<gpui_kit::FocusHandle> {
    all(inputs)
        .into_iter()
        .map(|input| input.read(cx).focus_handle(cx))
        .collect()
}

pub(super) fn fill(inputs: &FieldInputs, rules: &RenameRules, window: &mut Window, cx: &mut App) {
    let counter = rules.counter;
    let values = [
        rules.name_mask.clone(),
        rules.ext_mask.clone(),
        rules.find.text.clone(),
        rules.find.replace.clone(),
        counter.start.to_string(),
        counter.step.to_string(),
        counter.digits.to_string(),
    ];
    for (input, value) in all(inputs).into_iter().zip(values) {
        input.update(cx, |input, cx| input.set_value(value, window, cx));
    }
}

pub(super) fn watch(
    inputs: &FieldInputs,
    window: &mut Window,
    cx: &mut Context<FerriteWindow>,
) -> Vec<Subscription> {
    all(inputs)
        .into_iter()
        .map(|input| {
            cx.subscribe_in(input, window, |view, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    view.rules_changed(window, cx);
                }
            })
        })
        .collect()
}

pub(super) fn read(inputs: &FieldInputs, rules: &mut RenameRules, cx: &App) -> bool {
    let text = |input: &Entity<TextFieldState>| input.read(cx).value().to_string();
    rules.name_mask = text(&inputs.name_mask);
    rules.ext_mask = text(&inputs.ext_mask);
    rules.find.text = text(&inputs.find);
    rules.find.replace = text(&inputs.replace);
    let counter = (
        text(&inputs.start).trim().parse(),
        text(&inputs.step).trim().parse(),
        text(&inputs.digits).trim().parse(),
    );
    let (Ok(start), Ok(step), Ok(digits)) = counter else {
        return false;
    };
    rules.counter = Counter {
        start,
        step,
        digits,
    };
    true
}

pub(super) fn flip(find: &mut Find, toggle: Toggle) {
    let flag = match toggle {
        Toggle::Regex => &mut find.regex,
        Toggle::MatchCase => &mut find.match_case,
        Toggle::FirstOnly => &mut find.first_only,
        Toggle::NameOnly => &mut find.name_only,
    };
    *flag = !*flag;
}
