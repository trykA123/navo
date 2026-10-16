use super::{ItemId, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Run(ItemId),
    OpenSubmenu(Vec<usize>),
    ShowMore,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    index: Option<usize>,
    path: Option<Vec<usize>>,
}

impl Selection {
    #[must_use]
    pub fn first(rows: &[Row]) -> Self {
        Self::at(rows, step(rows, None, true))
    }

    #[must_use]
    pub fn index(&self) -> Option<usize> {
        self.index
    }

    #[must_use]
    pub fn path(&self) -> Option<&[usize]> {
        self.path.as_deref()
    }

    #[must_use]
    pub fn next(&self, rows: &[Row]) -> Self {
        Self::at(rows, step(rows, self.index, true))
    }

    #[must_use]
    pub fn previous(&self, rows: &[Row]) -> Self {
        Self::at(rows, step(rows, self.index, false))
    }

    #[must_use]
    pub fn keep(&self, old: &[Row], new: &[Row]) -> Self {
        let was_more = matches!(self.index.and_then(|i| old.get(i)), Some(Row::More { .. }));
        let found = new.iter().position(|row| match row {
            Row::Item(item) => self.path.as_deref() == Some(item.path.as_slice()),
            Row::More { .. } => was_more,
            Row::Header(_) | Row::Separator => false,
        });
        match found {
            Some(index) if new[index].is_selectable(!has_enabled(new)) => {
                Self::at(new, Some(index))
            }
            _ => Self::first(new),
        }
    }

    #[must_use]
    pub fn enter(&self, rows: &[Row]) -> Option<Choice> {
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
    pub fn open_submenu(&self, rows: &[Row]) -> Option<Vec<usize>> {
        match rows.get(self.index?)? {
            Row::Item(item) if item.enabled && item.opens_submenu => Some(item.path.clone()),
            _ => None,
        }
    }

    fn at(rows: &[Row], index: Option<usize>) -> Self {
        let path = index.and_then(|i| match rows.get(i) {
            Some(Row::Item(item)) => Some(item.path.clone()),
            _ => None,
        });
        Self { index, path }
    }
}

fn has_enabled(rows: &[Row]) -> bool {
    rows.iter().any(|row| row.is_selectable(false))
}

fn step(rows: &[Row], from: Option<usize>, forward: bool) -> Option<usize> {
    let len = rows.len();
    let allow_disabled = !has_enabled(rows);
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
