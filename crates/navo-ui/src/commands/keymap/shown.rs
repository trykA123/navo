use gpui_kit::Keystroke;

use super::parse::stroke;

#[must_use]
pub fn shown(key: &str) -> Option<String> {
    let strokes: Vec<String> = key
        .split_whitespace()
        .map(|part| stroke(part).map(|stroke| one(&stroke)))
        .collect::<Option<_>>()?;
    (!strokes.is_empty()).then(|| strokes.join(" "))
}

fn one(stroke: &Keystroke) -> String {
    let held = &stroke.modifiers;
    let modifiers = [
        (held.control, "Ctrl"),
        (held.alt, "Alt"),
        (held.shift, "Shift"),
        (held.platform, "Win"),
        (held.function, "Fn"),
    ];
    let mut parts: Vec<String> = modifiers
        .into_iter()
        .filter(|(is_held, _)| *is_held)
        .map(|(_, name)| name.to_owned())
        .collect();
    parts.push(capital(&stroke.key));
    parts.join("+")
}

fn capital(key: &str) -> String {
    let mut chars = key.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
