use crate::Location;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct History {
    back: Vec<Location>,
    forward: Vec<Location>,
}

impl History {
    pub const LIMIT: usize = 100;

    #[must_use]
    pub fn back(&self) -> &[Location] {
        &self.back
    }

    #[must_use]
    pub fn forward(&self) -> &[Location] {
        &self.forward
    }

    pub(crate) fn from_stacks(mut back: Vec<Location>, mut forward: Vec<Location>) -> Self {
        let excess = back.len().saturating_sub(Self::LIMIT);
        back.drain(..excess);
        forward.truncate(Self::LIMIT);
        Self { back, forward }
    }

    pub(super) fn contains(&self, location: &Location) -> bool {
        self.back.contains(location) || self.forward.contains(location)
    }

    pub(super) fn push(&mut self, current: Location) {
        self.back.push(current);
        if self.back.len() > Self::LIMIT {
            self.back.remove(0);
        }
        self.forward.clear();
    }

    pub(super) fn step_back(&mut self, current: Location) -> Option<Location> {
        let previous = self.back.pop()?;
        self.forward.insert(0, current);
        self.forward.truncate(Self::LIMIT);
        Some(previous)
    }

    pub(super) fn step_forward(&mut self, current: Location) -> Option<Location> {
        if self.forward.is_empty() {
            return None;
        }
        let next = self.forward.remove(0);
        self.back.push(current);
        if self.back.len() > Self::LIMIT {
            self.back.remove(0);
        }
        Some(next)
    }
}
