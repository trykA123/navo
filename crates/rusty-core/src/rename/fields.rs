use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use super::mask::{Mask, Token};
use super::rules::{RenameRules, RuleError, RuleField};

const FORBIDDEN: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    pub name: Cow<'static, str>,
    pub label_id: Cow<'static, str>,
}

impl FieldInfo {
    #[must_use]
    pub const fn builtin(name: &'static str, label_id: &'static str) -> Self {
        Self {
            name: Cow::Borrowed(name),
            label_id: Cow::Borrowed(label_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Values(BTreeMap<String, String>);

impl Values {
    pub fn insert(&mut self, field: &str, value: &str) {
        let safe: String = value
            .trim()
            .chars()
            .map(|ch| {
                if FORBIDDEN.contains(&ch) || ch.is_control() {
                    '_'
                } else {
                    ch
                }
            })
            .collect();
        if !safe.is_empty() {
            self.0.insert(field.to_owned(), safe);
        }
    }

    #[must_use]
    pub fn get(&self, field: &str) -> Option<&str> {
        self.0.get(field).map(String::as_str)
    }
}

pub trait FieldProvider: Send + Sync {
    fn group(&self) -> &str;
    fn fields(&self) -> &[FieldInfo];
    fn read(&self, path: &Path) -> Values;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldRef<'a> {
    pub group: &'a str,
    pub field: &'a FieldInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemFields(BTreeMap<String, Values>);

impl ItemFields {
    #[must_use]
    pub fn get(&self, group: &str, field: &str) -> Option<String> {
        self.0.get(group)?.get(field).map(str::to_owned)
    }
}

#[derive(Clone, Default)]
pub struct FieldRegistry {
    providers: Vec<Arc<dyn FieldProvider>>,
}

impl FieldRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn FieldProvider>) {
        match self
            .providers
            .iter_mut()
            .find(|known| known.group() == provider.group())
        {
            Some(known) => *known = provider,
            None => self.providers.push(provider),
        }
    }

    pub fn groups(&self) -> impl Iterator<Item = &str> {
        self.providers.iter().map(|provider| provider.group())
    }

    #[must_use]
    pub fn provider(&self, group: &str) -> Option<&Arc<dyn FieldProvider>> {
        self.providers
            .iter()
            .find(|provider| provider.group() == group)
    }

    pub fn listing(&self) -> impl Iterator<Item = FieldRef<'_>> {
        self.providers.iter().flat_map(|provider| {
            let group = provider.group();
            provider
                .fields()
                .iter()
                .map(move |field| FieldRef { group, field })
        })
    }

    #[must_use]
    pub fn read(&self, needed: &BTreeSet<String>, path: &Path) -> ItemFields {
        let read = needed.iter().filter_map(|group| {
            let provider = self.provider(group)?;
            Some((group.clone(), provider.read(path)))
        });
        ItemFields(read.collect())
    }
}

pub fn groups_needed(rules: &RenameRules) -> Result<BTreeSet<String>, RuleError> {
    let mut groups = BTreeSet::new();
    for (source, field) in [
        (&rules.name_mask, RuleField::NameMask),
        (&rules.ext_mask, RuleField::ExtMask),
    ] {
        let mask = Mask::parse(source).map_err(|e| RuleError {
            field,
            message: e.to_string(),
        })?;
        for token in mask.tokens() {
            if let Token::Field { group, .. } = token {
                groups.insert(group.clone());
            }
        }
    }
    Ok(groups)
}
