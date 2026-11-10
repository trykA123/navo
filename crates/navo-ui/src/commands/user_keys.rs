use std::rc::Rc;

use gpui_kit::{Action, App, Global, KeyBinding, KeyBindingContextPredicate};

use super::bind::with_shifted_digit_aliases;

/// Runs the user command with this name. It is data, so no keymap file can name it.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = navo_window, no_json)]
pub(crate) struct RunUserCommand {
    pub name: String,
}

const CONTEXT: &str = "FileList";

/// The keys of the user commands, as `(key, command name)`.
#[derive(Default)]
struct UserKeys(Vec<(String, String)>);

impl Global for UserKeys {}

#[must_use]
pub(crate) fn user_keys(cx: &App) -> Vec<(String, String)> {
    cx.try_global::<UserKeys>()
        .map_or_else(Vec::new, |keys| keys.0.clone())
}

pub(crate) fn set_user_keys(cx: &mut App, keys: Vec<(String, String)>) {
    cx.set_global(UserKeys(keys));
}

/// Binds the user command keys in `FileList`; they come last, so they win ties.
pub(crate) fn bind_user_command_keys(cx: &mut App) {
    let Ok(predicate) = KeyBindingContextPredicate::parse(CONTEXT) else {
        return;
    };
    let predicate = Rc::new(predicate);
    let bindings: Vec<KeyBinding> = user_keys(cx)
        .iter()
        .flat_map(|(key, name)| {
            with_shifted_digit_aliases(key)
                .into_iter()
                .map(move |key| (key, name.clone()))
        })
        .filter_map(|(key, name)| {
            let action = Box::new(RunUserCommand { name });
            // No key equivalents: the binding stays layout independent (see bind.rs).
            KeyBinding::load(&key, action, Some(predicate.clone()), false, None, cx).ok()
        })
        .collect();
    cx.bind_keys(bindings);
}
