use std::rc::Rc;

use gpui_kit::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};

use super::{Bind, GLOBAL, Keymap, active};

#[cfg(test)]
pub(crate) fn key_bindings(keymap: &Keymap, cx: &App, namespace: Option<&str>) -> Vec<KeyBinding> {
    bindings_where(
        cx,
        keymap,
        |key| vec![key.to_owned()],
        |bind| is_in(bind, namespace),
    )
}

pub(crate) fn bind_namespace(cx: &mut App, namespace: &str) {
    let keymap = active(cx);
    let bindings = bindings_where(cx, keymap, with_shifted_digit_aliases, |bind| {
        is_in(bind, Some(namespace)) && !keymap.is_user(bind.spec.name, &bind.context)
    });
    cx.bind_keys(bindings);
}

pub(crate) fn bind_user_keys(cx: &mut App) {
    let keymap = active(cx);
    let bindings = bindings_where(cx, keymap, with_shifted_digit_aliases, |bind| {
        is_in(bind, None) && keymap.is_user(bind.spec.name, &bind.context)
    });
    cx.bind_keys(bindings);
}

fn bindings_where(
    cx: &App,
    keymap: &Keymap,
    expand: fn(&str) -> Vec<String>,
    keep: impl Fn(&Bind) -> bool,
) -> Vec<KeyBinding> {
    keymap
        .binds()
        .iter()
        .filter(|bind| keep(bind))
        .flat_map(|bind| bindings_of(bind, cx, expand))
        .collect()
}

fn is_in(bind: &Bind, namespace: Option<&str>) -> bool {
    let Some(action) = bind.spec.action else {
        return false;
    };
    namespace.is_none_or(|namespace| {
        action
            .split_once("::")
            .is_some_and(|(space, _)| space == namespace)
    })
}

fn bindings_of(bind: &Bind, cx: &App, expand: fn(&str) -> Vec<String>) -> Vec<KeyBinding> {
    let Some(action) = bind
        .spec
        .action
        .and_then(|name| cx.build_action(name, None).ok())
    else {
        return Vec::new();
    };
    let predicate = match bind.context.as_str() {
        GLOBAL => None,
        context => match KeyBindingContextPredicate::parse(context) {
            Ok(predicate) => Some(Rc::new(predicate)),
            Err(_) => return Vec::new(),
        },
    };
    bind.keys
        .iter()
        .flat_map(|key| expand(key))
        .filter_map(|key| {
            let action = action.boxed_clone();
            let mapper = &DummyKeyboardMapper;
            KeyBinding::load(&key, action, predicate.clone(), false, None, mapper).ok()
        })
        .collect()
}

#[cfg(target_os = "linux")]
const SHIFTED_DIGITS: [(&str, usize); 5] = [("!", 1), ("@", 2), ("\"", 2), ("#", 3), ("$", 4)];

#[cfg(target_os = "linux")]
pub(crate) fn shifted_digit(symbol: &str) -> Option<usize> {
    SHIFTED_DIGITS
        .iter()
        .find_map(|(shifted, digit)| (*shifted == symbol).then_some(*digit))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn shifted_digit(_symbol: &str) -> Option<usize> {
    None
}

#[cfg(target_os = "linux")]
fn with_shifted_digit_aliases(key: &str) -> Vec<String> {
    let digit = key
        .strip_prefix("ctrl-shift-")
        .and_then(|digit| digit.parse::<usize>().ok());
    let aliases = SHIFTED_DIGITS
        .iter()
        .filter(move |(_, shifted)| Some(*shifted) == digit)
        .map(|(symbol, _)| format!("ctrl-{symbol}"));
    std::iter::once(key.to_owned()).chain(aliases).collect()
}

#[cfg(not(target_os = "linux"))]
fn with_shifted_digit_aliases(key: &str) -> Vec<String> {
    vec![key.to_owned()]
}

#[cfg(test)]
pub(super) fn is_shifted_digit_alias(keys: &str) -> bool {
    (1..=4).any(|digit| {
        with_shifted_digit_aliases(&format!("ctrl-shift-{digit}"))
            .iter()
            .skip(1)
            .any(|alias| alias == keys)
    })
}
