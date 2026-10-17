use std::rc::Rc;

use gpui_kit::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};

use super::{Bind, GLOBAL, Keymap, active};

pub(crate) fn key_bindings(keymap: &Keymap, cx: &App, namespace: Option<&str>) -> Vec<KeyBinding> {
    keymap
        .binds()
        .iter()
        .filter(|bind| is_in(bind, namespace))
        .flat_map(|bind| bindings_of(bind, cx))
        .collect()
}

pub(crate) fn bind_namespace(cx: &mut App, namespace: &str) {
    let bindings = key_bindings(active(cx), cx, Some(namespace));
    cx.bind_keys(bindings);
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

fn bindings_of(bind: &Bind, cx: &App) -> Vec<KeyBinding> {
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
        .filter_map(|key| {
            let action = action.boxed_clone();
            let mapper = &DummyKeyboardMapper;
            KeyBinding::load(key, action, predicate.clone(), false, None, mapper).ok()
        })
        .collect()
}
