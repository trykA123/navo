use std::collections::HashMap;

pub const KIND_LIMIT: usize = 64;
const WORKING_LIMIT: usize = 4 * KIND_LIMIT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    pub bytes: u64,
    pub files: u64,
}

impl Tally {
    fn add(&mut self, other: Self) {
        self.bytes = self.bytes.saturating_add(other.bytes);
        self.files = self.files.saturating_add(other.files);
    }
}

#[derive(Debug, Default)]
pub(super) struct Kinds {
    kinds: HashMap<String, Tally>,
    other: Tally,
}

impl Kinds {
    pub(super) fn add(&mut self, name: &str, bytes: u64) {
        let file = Tally { bytes, files: 1 };
        let extension = extension_of(name);
        if let Some(tally) = self.kinds.get_mut(&extension) {
            tally.add(file);
            return;
        }
        self.kinds.insert(extension, file);
        if self.kinds.len() > WORKING_LIMIT {
            self.fold_smallest();
        }
    }

    fn fold_smallest(&mut self) {
        let smallest = self
            .kinds
            .iter()
            .min_by(|a, b| a.1.bytes.cmp(&b.1.bytes).then_with(|| b.0.cmp(a.0)))
            .map(|(name, _)| name.clone());
        if let Some(tally) = smallest.and_then(|name| self.kinds.remove(&name)) {
            self.other.add(tally);
        }
    }

    pub(super) fn snapshot(&self) -> (Vec<(String, Tally)>, Tally) {
        let mut kinds: Vec<(String, Tally)> = self
            .kinds
            .iter()
            .map(|(name, tally)| (name.clone(), *tally))
            .collect();
        kinds.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then_with(|| a.0.cmp(&b.0)));
        let mut other = self.other;
        kinds
            .drain(KIND_LIMIT.min(kinds.len())..)
            .for_each(|(_, tally)| other.add(tally));
        (kinds, other)
    }
}

fn extension_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => extension.to_lowercase(),
        _ => String::new(),
    }
}
