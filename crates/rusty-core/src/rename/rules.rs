use serde::{Deserialize, Serialize};

use super::case::{ExtCase, NameCase};
use super::mask::{CounterSpec, Mask, Subject};
use super::search::Search;
use super::split::split_name;

pub const RULES_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Find {
    pub text: String,
    pub replace: String,
    pub regex: bool,
    pub match_case: bool,
    pub first_only: bool,
    pub name_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Counter {
    pub start: u64,
    pub step: u64,
    pub digits: u8,
}

impl Default for Counter {
    fn default() -> Self {
        Self {
            start: 1,
            step: 1,
            digits: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenameRules {
    pub version: u32,
    pub name_mask: String,
    pub ext_mask: String,
    pub find: Find,
    pub name_case: NameCase,
    pub ext_case: ExtCase,
    pub counter: Counter,
}

impl Default for RenameRules {
    fn default() -> Self {
        Self {
            version: RULES_VERSION,
            name_mask: "[N]".to_owned(),
            ext_mask: "[E]".to_owned(),
            find: Find::default(),
            name_case: NameCase::Unchanged,
            ext_case: ExtCase::Unchanged,
            counter: Counter::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleField {
    NameMask,
    ExtMask,
    Find,
    Counter,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RuleError {
    pub field: RuleField,
    pub message: String,
}

pub struct Compiled {
    name_mask: Mask,
    ext_mask: Mask,
    search: Search,
    name_only: bool,
    name_case: NameCase,
    ext_case: ExtCase,
    counter: CounterSpec,
}

impl Compiled {
    pub fn new(rules: &RenameRules) -> Result<Self, RuleError> {
        let fail = |field, message: String| RuleError { field, message };
        Ok(Self {
            name_mask: Mask::parse(&rules.name_mask)
                .map_err(|e| fail(RuleField::NameMask, e.to_string()))?,
            ext_mask: Mask::parse(&rules.ext_mask)
                .map_err(|e| fail(RuleField::ExtMask, e.to_string()))?,
            search: Search::new(&rules.find).map_err(|m| fail(RuleField::Find, m))?,
            name_only: rules.find.name_only,
            name_case: rules.name_case,
            ext_case: rules.ext_case,
            counter: counter(rules.counter).map_err(|m| fail(RuleField::Counter, m))?,
        })
    }

    #[must_use]
    pub fn new_name(&self, subject: &Subject<'_>, index: u64) -> String {
        let stem = self.name_mask.render_with(subject, index, self.counter);
        let ext = self.ext_mask.render_with(subject, index, self.counter);
        let (stem, ext) = if self.name_only {
            (self.search.apply(&stem), ext)
        } else {
            let whole = self.search.apply(&join(&stem, &ext));
            let (stem, ext) = split_name(&whole);
            (stem.to_owned(), ext.to_owned())
        };
        join(&self.name_case.apply(&stem), &self.ext_case.apply(&ext))
    }
}

fn counter(counter: Counter) -> Result<CounterSpec, String> {
    if counter.step < 1 {
        return Err("step must be at least 1".to_owned());
    }
    if !(1..=10).contains(&counter.digits) {
        return Err("digits must be from 1 to 10".to_owned());
    }
    Ok(CounterSpec {
        start: counter.start,
        step: counter.step,
        digits: counter.digits,
    })
}

fn join(stem: &str, ext: &str) -> String {
    if ext.is_empty() {
        stem.to_owned()
    } else {
        format!("{stem}.{ext}")
    }
}
