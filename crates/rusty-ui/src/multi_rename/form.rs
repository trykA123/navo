use std::collections::BTreeSet;

use rusty_core::rename::{Compiled, ExtCase, Find, NameCase, RenameRules, RuleError};

use super::item::Item;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Form {
    pub rules: RenameRules,
    pub problems_only: bool,
    excluded: BTreeSet<usize>,
}

impl Form {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_preset(rules: &RenameRules) -> Self {
        Self {
            rules: rules.clone(),
            ..Self::default()
        }
    }

    pub fn load_preset(&mut self, rules: &RenameRules) {
        self.rules = rules.clone();
    }

    #[must_use]
    pub fn rules(&self) -> RenameRules {
        self.rules.clone()
    }

    pub fn compile(&self) -> Result<Compiled, RuleError> {
        Compiled::new(&self.rules)
    }

    pub fn reset(&mut self) {
        self.rules = RenameRules::default();
    }

    pub fn set_name_mask(&mut self, text: impl Into<String>) {
        self.rules.name_mask = text.into();
    }

    pub fn set_ext_mask(&mut self, text: impl Into<String>) {
        self.rules.ext_mask = text.into();
    }

    pub fn set_find(&mut self, find: Find) {
        self.rules.find = find;
    }

    pub fn set_cases(&mut self, name: NameCase, ext: ExtCase) {
        self.rules.name_case = name;
        self.rules.ext_case = ext;
    }

    pub fn toggle_excluded(&mut self, row: usize) {
        if !self.excluded.remove(&row) {
            self.excluded.insert(row);
        }
    }

    #[must_use]
    pub fn is_excluded(&self, row: usize) -> bool {
        self.excluded.contains(&row)
    }

    pub fn clear_excluded(&mut self) {
        self.excluded.clear();
    }

    pub fn mark_excluded(&self, items: &mut [Item]) {
        for (index, item) in items.iter_mut().enumerate() {
            item.excluded = self.excluded.contains(&index);
        }
    }
}
