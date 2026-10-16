use rusty_core::shell_menu::{Group, Icon, ItemId, ItemKind, MenuItem, Submenu, clean_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RawItem {
    pub command: u32,
    pub text: String,
    pub verb: Option<String>,
    pub enabled: bool,
    pub checked: bool,
    pub default: bool,
    pub separator: bool,
    pub icon: Option<Icon>,
}

const OPEN_VERBS: &[&str] = &["open", "openas", "edit", "runas"];
const OPEN_TEXTS: &[&str] = &["open with"];
const EDIT_VERBS: &[&str] = &["copy", "cut", "delete", "rename", "paste", "link"];
const SHARE_VERBS: &[&str] = &["windows.modernshare", "sendto", "share"];
const SHARE_TEXTS: &[&str] = &["send to", "give access to", "share"];

pub(super) fn guess_group(verb: Option<&str>, text: &str) -> Group {
    let verb = verb.map(str::to_ascii_lowercase).unwrap_or_default();
    let text = clean_text(text).text.to_lowercase();
    let is_open = OPEN_VERBS.contains(&verb.as_str()) || verb.starts_with("open");
    if is_open || OPEN_TEXTS.contains(&text.as_str()) {
        Group::Open
    } else if EDIT_VERBS.contains(&verb.as_str()) {
        Group::Edit
    } else if SHARE_VERBS.contains(&verb.as_str()) || SHARE_TEXTS.contains(&text.as_str()) {
        Group::Share
    } else {
        Group::Extensions
    }
}

pub(super) fn is_unfilled(children: &[RawItem]) -> bool {
    children
        .iter()
        .filter(|child| !child.separator)
        .all(|child| clean_text(&child.text).text.is_empty())
}

pub(super) fn to_menu_item(
    raw: RawItem,
    submenu: Option<Submenu>,
    parent: Option<Group>,
) -> MenuItem {
    let group = parent.unwrap_or_else(|| guess_group(raw.verb.as_deref(), &raw.text));
    if raw.separator {
        return MenuItem::separator(group);
    }
    let id = ItemId::Shell {
        command: raw.command,
        verb: raw.verb,
    };
    MenuItem {
        icon: raw.icon,
        enabled: raw.enabled,
        checked: raw.checked,
        default: raw.default,
        kind: submenu.map_or(ItemKind::Command, ItemKind::Submenu),
        ..MenuItem::new(id, &raw.text, group)
    }
}

#[cfg(test)]
mod tests;
