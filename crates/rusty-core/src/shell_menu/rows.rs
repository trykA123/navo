use std::ops::Range;

use super::{ItemId, MenuItem, ShellMenu, filter};

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
    Back { title: String },
    Separator,
    Item(ItemRow),
}

impl Row {
    #[must_use]
    pub fn is_selectable(&self, allow_disabled: bool) -> bool {
        match self {
            Self::Item(item) => item.enabled || allow_disabled,
            Self::Back { .. } | Self::Separator => false,
        }
    }
}

/// The rows of one page: the root for an empty `page`, else the submenu at `page` under a back row.
#[must_use]
pub fn rows(menu: &ShellMenu, page: &[usize], query: &str) -> Vec<Row> {
    let (mut out, items) = if page.is_empty() {
        (Vec::new(), Some(menu.items.as_slice()))
    } else {
        let Some(item) = menu.item(page) else {
            return Vec::new();
        };
        let back = Row::Back {
            title: item.text.clone(),
        };
        (vec![back], item.children())
    };
    let Some(items) = items else {
        return out;
    };
    if query.trim().is_empty() {
        out.extend(listed(items, page));
    } else {
        out.extend(filter::matches(items, page, query));
    }
    out
}

fn listed(items: &[MenuItem], page: &[usize]) -> Vec<Row> {
    let mut out = Vec::new();
    let mut pending = false;
    for (index, item) in items.iter().enumerate() {
        if item.is_separator() {
            pending = true;
            continue;
        }
        if item.hidden || item.text.is_empty() {
            continue;
        }
        if pending && !out.is_empty() {
            out.push(Row::Separator);
        }
        pending = false;
        out.push(Row::Item(ItemRow::new([page, &[index]].concat(), item)));
    }
    out
}
