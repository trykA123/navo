#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ring {
    pub tiles: usize,
    pub rows: usize,
    pub columns: usize,
}

impl Ring {
    fn len(self) -> usize {
        self.tiles + self.rows
    }

    #[must_use]
    pub fn step(self, from: usize, step: Step) -> usize {
        let len = self.len();
        if len == 0 {
            return 0;
        }
        let from = from.min(len - 1);
        match step {
            Step::Right => (from + 1) % len,
            Step::Left => (from + len - 1) % len,
            Step::Down => self.down(from),
            Step::Up => self.up(from),
        }
    }

    fn down(self, from: usize) -> usize {
        let columns = self.columns.max(1);
        if from < self.tiles && from + columns < self.tiles {
            return from + columns;
        }
        if from < self.tiles {
            return self.tiles % self.len();
        }
        (from + 1) % self.len()
    }

    fn up(self, from: usize) -> usize {
        let columns = self.columns.max(1);
        if from < self.tiles && from >= columns {
            return from - columns;
        }
        if from < self.tiles || from == 0 {
            return self.len() - 1;
        }
        from - 1
    }
}

#[must_use]
pub fn jump(labels: &[&str], from: Option<usize>, key: &str) -> Option<usize> {
    let mut chars = key.chars();
    let (Some(typed), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if !typed.is_alphanumeric() {
        return None;
    }
    let starts = |label: &&str| {
        label
            .chars()
            .next()
            .is_some_and(|first| first.to_lowercase().eq(typed.to_lowercase()))
    };
    let start = from.map_or(0, |index| index + 1);
    (0..labels.len())
        .map(|offset| (start + offset) % labels.len())
        .find(|index| labels.get(*index).is_some_and(starts))
}
