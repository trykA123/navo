use super::results::Row;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection(Option<usize>);

impl Selection {
    #[must_use]
    pub fn start(rows: &[Row]) -> Self {
        Self(selectable(rows).next())
    }

    #[must_use]
    pub fn at(rows: &[Row], index: usize) -> Self {
        let is_selectable = rows.get(index).is_some_and(Row::is_selectable);
        Self(is_selectable.then_some(index))
    }

    #[must_use]
    pub fn index(self) -> Option<usize> {
        self.0
    }

    #[must_use]
    pub fn row(self, rows: &[Row]) -> Option<&Row> {
        self.0.and_then(|index| rows.get(index))
    }

    pub fn step(&mut self, rows: &[Row], delta: isize) {
        let stops: Vec<usize> = selectable(rows).collect();
        let Some(last) = stops.len().checked_sub(1) else {
            self.0 = None;
            return;
        };
        let current = self
            .0
            .and_then(|index| stops.iter().position(|stop| *stop == index));
        let target = current.map_or(0, |at| at.saturating_add_signed(delta).min(last));
        self.0 = Some(stops[target]);
    }

    pub fn first(&mut self, rows: &[Row]) {
        *self = Self::start(rows);
    }

    pub fn last(&mut self, rows: &[Row]) {
        self.0 = selectable(rows).last();
    }

    pub fn next_group(&mut self, rows: &[Row]) {
        self.jump(rows, |current, count| (current + 1) % count);
    }

    pub fn previous_group(&mut self, rows: &[Row]) {
        self.jump(rows, |current, count| (current + count - 1) % count);
    }

    fn jump(&mut self, rows: &[Row], target: impl Fn(usize, usize) -> usize) {
        let starts = group_starts(rows);
        if starts.is_empty() {
            self.0 = None;
            return;
        }
        let current = self.0.map_or(0, |index| {
            starts
                .iter()
                .rposition(|start| *start <= index)
                .unwrap_or(0)
        });
        self.0 = Some(starts[target(current, starts.len())]);
    }
}

fn selectable(rows: &[Row]) -> impl Iterator<Item = usize> + '_ {
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| row.is_selectable().then_some(index))
}

fn group_starts(rows: &[Row]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut fresh = true;
    for (index, row) in rows.iter().enumerate() {
        match row {
            Row::Header(_) => fresh = true,
            row if row.is_selectable() && fresh => {
                starts.push(index);
                fresh = false;
            }
            _ => {}
        }
    }
    starts
}
