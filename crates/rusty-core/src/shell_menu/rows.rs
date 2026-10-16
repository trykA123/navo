use std::ops::Range;

use super::{ItemId, MenuItem, ShellMenu, filter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Ferrite,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    pub path: Vec<usize>,
    pub id: ItemId,
    pub text: String,
    pub crumb: Vec<String>,
    pub matched: Option<Range<usize>>,
    pub enabled: bool,
    pub hidden: bool,
    pub opens_submenu: bool,
}

impl ItemRow {
    pub(super) fn new(path: Vec<usize>, item: &MenuItem) -> Self {
        Self {
            path,
            id: item.id.clone(),
            text: item.text.clone(),
            crumb: Vec::new(),
            matched: None,
            enabled: item.enabled,
            hidden: item.hidden,
            opens_submenu: item.is_submenu(),
        }
    }

    #[must_use]
    pub fn breadcrumb(&self) -> String {
        let mut parts: Vec<&str> = self.crumb.iter().map(String::as_str).collect();
        parts.push(&self.text);
        parts.join(" › ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Header(Section),
    Separator,
    Item(ItemRow),
    More { count: usize },
}

impl Row {
    #[must_use]
    pub fn is_selectable(&self, allow_disabled: bool) -> bool {
        match self {
            Self::Item(item) => item.enabled || allow_disabled,
            Self::More { .. } => true,
            Self::Header(_) | Self::Separator => false,
        }
    }
}

#[must_use]
pub fn rows(menu: &ShellMenu, query: &str) -> Vec<Row> {
    if query.trim().is_empty() {
        overview(menu)
    } else {
        filter::matches(menu, query)
    }
}

#[must_use]
pub fn submenu_rows(menu: &ShellMenu, path: &[usize]) -> Vec<Row> {
    let Some(children) = menu.item(path).and_then(MenuItem::children) else {
        return Vec::new();
    };
    with_separators(
        children
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.hidden)
            .map(|(index, item)| ([path, &[index]].concat(), item)),
    )
}

#[must_use]
pub fn rare_rows(menu: &ShellMenu) -> Vec<Row> {
    with_separators(
        top_level(menu).filter(|(_, item)| item.rare && !item.is_ferrite() && !item.hidden),
    )
}

fn overview(menu: &ShellMenu) -> Vec<Row> {
    let mut out = Vec::new();
    let ferrite =
        with_separators(top_level(menu).filter(|(_, item)| item.is_ferrite() && !item.hidden));
    if !ferrite.is_empty() {
        out.push(Row::Header(Section::Ferrite));
        out.extend(ferrite);
    }
    let shell = with_separators(
        top_level(menu).filter(|(_, item)| !item.is_ferrite() && !item.hidden && !item.rare),
    );
    let rare = rare_rows(menu)
        .iter()
        .filter(|row| matches!(row, Row::Item(_)))
        .count();
    if shell.is_empty() && rare == 0 {
        return out;
    }
    out.push(Row::Header(Section::Windows));
    let has_shell = !shell.is_empty();
    out.extend(shell);
    if rare > 0 {
        if has_shell {
            out.push(Row::Separator);
        }
        out.push(Row::More { count: rare });
    }
    out
}

fn top_level(menu: &ShellMenu) -> impl Iterator<Item = (Vec<usize>, &MenuItem)> {
    menu.items
        .iter()
        .enumerate()
        .map(|(index, item)| (vec![index], item))
}

fn with_separators<'a>(items: impl Iterator<Item = (Vec<usize>, &'a MenuItem)>) -> Vec<Row> {
    let mut out = Vec::new();
    let mut pending = false;
    let mut last_group = None;
    for (path, item) in items {
        if item.is_separator() {
            pending = true;
            continue;
        }
        if item.text.is_empty() {
            continue;
        }
        if last_group.is_some_and(|group| group != item.group) {
            pending = true;
        }
        if pending && !out.is_empty() {
            out.push(Row::Separator);
        }
        pending = false;
        last_group = Some(item.group);
        out.push(Row::Item(ItemRow::new(path, item)));
    }
    out
}
