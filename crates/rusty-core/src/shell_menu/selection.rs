use super::{ItemId, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Run(ItemId),
    OpenSubmenu(Vec<usize>),
    ShowMore,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    index: Option<usize>,
}

impl Selection {
    #[must_use]
    pub fn first(rows: &[Row]) -> Self {
        Self {
            index: step(rows, None, true),
        }
    }

    #[must_use]
    pub fn index(self) -> Option<usize> {
        self.index
    }

    #[must_use]
    pub fn next(self, rows: &[Row]) -> Self {
        Self {
            index: step(rows, self.index, true),
        }
    }

    #[must_use]
    pub fn previous(self, rows: &[Row]) -> Self {
        Self {
            index: step(rows, self.index, false),
        }
    }

    #[must_use]
    pub fn enter(self, rows: &[Row]) -> Option<Choice> {
        match rows.get(self.index?)? {
            Row::Item(item) if item.enabled && item.opens_submenu => {
                Some(Choice::OpenSubmenu(item.path.clone()))
            }
            Row::Item(item) if item.enabled => Some(Choice::Run(item.id.clone())),
            Row::More { .. } => Some(Choice::ShowMore),
            _ => None,
        }
    }

    #[must_use]
    pub fn open_submenu(self, rows: &[Row]) -> Option<Vec<usize>> {
        match rows.get(self.index?)? {
            Row::Item(item) if item.enabled && item.opens_submenu => Some(item.path.clone()),
            _ => None,
        }
    }
}

fn step(rows: &[Row], from: Option<usize>, forward: bool) -> Option<usize> {
    let len = rows.len();
    let allow_disabled = !rows.iter().any(|row| row.is_selectable(false));
    let start = match (from.filter(|index| *index < len), forward) {
        (Some(index), true) => index + 1,
        (Some(index), false) => index + len.checked_sub(1)?,
        (None, true) => 0,
        (None, false) => len.checked_sub(1)?,
    };
    (0..len)
        .map(|offset| {
            if forward {
                (start + offset) % len
            } else {
                (start % len + len - offset) % len
            }
        })
        .find(|index| rows[*index].is_selectable(allow_disabled))
}
