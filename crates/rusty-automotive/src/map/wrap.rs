use super::MAX_MAP_LINE;

pub(super) struct Wrapped {
    cells: Vec<String>,
    filled: Vec<bool>,
    pub line: u64,
}

fn is_filled(raw: &str) -> bool {
    !raw.is_empty() && !raw.ends_with(char::is_whitespace)
}

impl Wrapped {
    pub(super) fn start(raw: &[&str], line: u64) -> Option<Self> {
        let filled: Vec<bool> = raw.iter().map(|cell| is_filled(cell)).collect();
        filled.contains(&true).then(|| Self {
            cells: raw.iter().map(|cell| cell.trim().to_owned()).collect(),
            filled,
            line,
        })
    }

    pub(super) fn extend(&mut self, raw: &[&str]) -> bool {
        let fits = raw.len() == self.cells.len()
            && raw.iter().any(|cell| !cell.trim().is_empty())
            && raw
                .iter()
                .zip(&self.filled)
                .zip(&self.cells)
                .all(|((cell, &filled), held)| {
                    let text = cell.trim();
                    text.is_empty() || (filled && held.len() + text.len() <= MAX_MAP_LINE)
                });
        if !fits {
            return false;
        }
        for (held, cell) in self.cells.iter_mut().zip(raw) {
            held.push_str(cell.trim());
        }
        self.filled = raw.iter().map(|cell| is_filled(cell)).collect();
        true
    }

    pub(super) fn is_open(&self) -> bool {
        self.filled.contains(&true)
    }

    pub(super) fn cells(&self) -> impl Iterator<Item = &str> {
        self.cells.iter().map(String::as_str)
    }
}
