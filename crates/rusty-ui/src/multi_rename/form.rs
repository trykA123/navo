use std::collections::BTreeSet;

use rusty_core::rename::{Compiled, ExtCase, Find, NameCase, RenameRules, RuleError};

use super::item::Item;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Form {
    pub rules: RenameRules,
    pub problems_only: bool,
    pub counter_per_folder: bool,
    excluded: BTreeSet<usize>,
    edited_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameCount {
    pub expected: usize,
    pub actual: usize,
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
        self.counter_per_folder = false;
        self.clear_edited_names();
    }

    pub fn set_edited_names(&mut self, text: &str) {
        self.edited_names = Some(text.lines().map(str::to_owned).collect());
    }

    #[must_use]
    pub fn edited_names(&self) -> Option<&[String]> {
        self.edited_names.as_deref()
    }

    #[must_use]
    pub fn has_edited_names(&self) -> bool {
        self.edited_names.is_some()
    }

    pub fn clear_edited_names(&mut self) {
        self.edited_names = None;
    }

    #[must_use]
    pub fn names_error(&self, item_count: usize) -> Option<NameCount> {
        let actual = self.edited_names()?.len();
        (actual != item_count).then_some(NameCount {
            expected: item_count,
            actual,
        })
    }

    pub fn reorder(&mut self, order: &[usize]) -> bool {
        let indices: BTreeSet<usize> = order.iter().copied().collect();
        if indices.len() != order.len()
            || indices.last().is_some_and(|&index| index >= order.len())
            || self
                .excluded
                .last()
                .is_some_and(|&index| index >= order.len())
            || self.names_error(order.len()).is_some()
        {
            return false;
        }
        self.excluded = order
            .iter()
            .enumerate()
            .filter_map(|(new, old)| self.is_excluded(*old).then_some(new))
            .collect();
        if let Some(names) = &mut self.edited_names {
            *names = order.iter().map(|&old| names[old].clone()).collect();
        }
        true
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
