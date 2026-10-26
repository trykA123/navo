use rusty_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};

pub(super) const PROPERTIES_VERB: &str = "properties";

/// Navo's own items around the shell list: `top` above it, `after_new` where Explorer shows New ▸, `hidden` reached by typing.
#[derive(Clone, Default)]
pub(super) struct Own {
    pub top: Vec<MenuItem>,
    pub after_new: Vec<MenuItem>,
    pub hidden: Vec<MenuItem>,
}

pub(super) struct Merged {
    pub menu: ShellMenu,
    pub shell_at: Vec<Option<usize>>,
}

pub(super) fn shell_path(shell_at: &[Option<usize>], path: &[usize]) -> Option<Vec<usize>> {
    let (first, rest) = path.split_first()?;
    let at = (*shell_at.get(*first)?)?;
    Some([&[at], rest].concat())
}

pub(super) fn merge(shell: &[MenuItem], own: &Own) -> Merged {
    let mut merged = Merged {
        menu: ShellMenu::default(),
        shell_at: Vec::new(),
    };
    let mut push = |item: MenuItem, at: Option<usize>| {
        merged.menu.items.push(item);
        merged.shell_at.push(at);
    };
    own.top.iter().for_each(|item| push(item.clone(), None));
    if !own.top.is_empty() {
        push(MenuItem::separator(Group::Ferrite), None);
    }
    let new_at = new_spot(shell);
    for (index, item) in shell.iter().enumerate() {
        if index == new_at {
            own.after_new
                .iter()
                .for_each(|item| push(item.clone(), None));
        }
        push(item.clone(), Some(index));
    }
    if new_at >= shell.len() {
        own.after_new
            .iter()
            .for_each(|item| push(item.clone(), None));
    }
    for item in &own.hidden {
        let hidden = MenuItem {
            hidden: true,
            ..item.clone()
        };
        push(hidden, None);
    }
    merged
}

/// Explorer lists New ▸ just above the separator that leads to Properties.
fn new_spot(shell: &[MenuItem]) -> usize {
    let Some(properties) = shell.iter().position(is_properties) else {
        return shell.len();
    };
    shell[..properties]
        .iter()
        .rposition(MenuItem::is_separator)
        .unwrap_or(properties)
}

fn is_properties(item: &MenuItem) -> bool {
    matches!(&item.id, ItemId::Shell { verb: Some(verb), .. } if verb.eq_ignore_ascii_case(PROPERTIES_VERB))
}

pub(super) fn mark_loading(items: &mut [MenuItem], parent: &[usize]) -> Vec<Vec<usize>> {
    let mut lazy = Vec::new();
    for (index, item) in items.iter_mut().enumerate() {
        let path = [parent, &[index]].concat();
        match &mut item.kind {
            ItemKind::Submenu(submenu @ Submenu::Lazy) => {
                *submenu = Submenu::Loading;
                lazy.push(path);
            }
            ItemKind::Submenu(Submenu::Filled(children)) => {
                lazy.extend(mark_loading(children, &path));
            }
            _ => {}
        }
    }
    lazy
}
