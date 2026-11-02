use std::collections::HashMap;

use super::{Location, ProviderPath};

pub struct AncestorMemo<F> {
    hit: F,
    answers: HashMap<Location, bool>,
}

impl<F: Fn(&Location, Option<&ProviderPath>) -> bool> AncestorMemo<F> {
    pub fn new(hit: F) -> Self {
        Self {
            hit,
            answers: HashMap::new(),
        }
    }

    pub fn any(&mut self, location: &Location) -> bool {
        if let Some(&known) = self.answers.get(location) {
            return known;
        }
        let (parent, name) = location.parent_and_name();
        (self.hit)(location, name.as_ref()) || parent.is_some_and(|parent| self.any_owned(parent))
    }

    fn any_owned(&mut self, location: Location) -> bool {
        let mut chain = Vec::new();
        let mut next = Some(location);
        let mut answer = loop {
            let Some(level) = next else {
                break false;
            };
            if let Some(&known) = self.answers.get(&level) {
                break known;
            }
            let (parent, name) = level.parent_and_name();
            let hit = (self.hit)(&level, name.as_ref());
            chain.push((level, hit));
            next = parent;
        };
        for (level, hit) in chain.into_iter().rev() {
            answer |= hit;
            self.answers.insert(level, answer);
        }
        answer
    }
}
