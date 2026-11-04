//! Checks the key of a user command.

use navo_ui::commands::normal_key;

const MODIFIERS: [&str; 10] = [
    "ctrl", "control", "alt", "shift", "fn", "function", "platform", "win", "super", "cmd",
];

pub(super) fn normalize_key(key: &str) -> Result<String, String> {
    if key.split_whitespace().count() != 1 {
        return Err("a shortcut must be one key".to_owned());
    }
    let normal = normal_key(key).ok_or_else(|| format!("`{key}` is not a key"))?;
    let (key_name, modifiers) =
        split_normal(&normal).ok_or_else(|| format!("`{key}` is not a key"))?;
    if MODIFIERS.contains(&key_name) {
        return Err(format!("`{key}` is a modifier alone; add another key"));
    }
    if bare(key_name, &modifiers) {
        return Err(format!(
            "`{key}` is a bare letter or digit; add Ctrl or Alt"
        ));
    }
    if modifiers
        .iter()
        .any(|modifier| matches!(*modifier, "win" | "super" | "cmd" | "platform"))
        || os_reserved(key_name, &modifiers)
    {
        return Err(format!("`{key}` is reserved by the operating system"));
    }
    Ok(normal)
}

fn split_normal(normal: &str) -> Option<(&str, Vec<&str>)> {
    if normal == "-" {
        return Some(("-", Vec::new()));
    }
    let mut parts: Vec<&str> = normal.split('-').collect();
    let key = parts.pop()?;
    if key.is_empty() {
        return None;
    }
    Some((key, parts))
}

fn bare(key: &str, modifiers: &[&str]) -> bool {
    let mut chars = key.chars();
    let single = matches!(chars.next(), Some(ch) if ch.is_alphanumeric()) && chars.next().is_none();
    single && modifiers.iter().all(|modifier| *modifier == "shift")
}

fn os_reserved(key: &str, modifiers: &[&str]) -> bool {
    let held = |modifier: &str| modifiers.contains(&modifier);
    match key {
        "tab" | "space" => held("alt"),
        "escape" => held("alt") || held("ctrl"),
        "f4" => held("alt"),
        "delete" => held("ctrl") && held("alt"),
        _ => false,
    }
}
