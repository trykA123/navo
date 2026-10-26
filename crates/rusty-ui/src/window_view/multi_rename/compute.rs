use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusty_core::ProviderPath;
use rusty_core::rename::check::{Existing, Fold};
use rusty_core::rename::{FieldRegistry, ItemFields, groups_needed};

use crate::multi_rename::{Form, Item, Listing, Preview};

pub(super) const DEBOUNCE: Duration = Duration::from_millis(60);

#[derive(Default)]
pub(super) struct FieldCache {
    groups: BTreeSet<String>,
    values: Vec<ItemFields>,
}

pub(super) struct Source {
    pub items: Arc<Vec<Item>>,
    pub existing: Arc<Existing>,
    pub fold: Fold,
    pub registry: Arc<FieldRegistry>,
    pub fields: Arc<Mutex<FieldCache>>,
}

impl Source {
    pub(super) fn job(&self, form: &Form, generation: u64) -> Job {
        Job {
            form: form.clone(),
            items: Arc::clone(&self.items),
            existing: Arc::clone(&self.existing),
            fold: self.fold,
            registry: Arc::clone(&self.registry),
            fields: Arc::clone(&self.fields),
            generation,
        }
    }
}

pub(super) struct Job {
    form: Form,
    items: Arc<Vec<Item>>,
    existing: Arc<Existing>,
    fold: Fold,
    registry: Arc<FieldRegistry>,
    fields: Arc<Mutex<FieldCache>>,
    generation: u64,
}

impl Job {
    pub(super) fn run(self) -> Option<Preview> {
        let needed = groups_needed(&self.form.rules).ok()?;
        let mut items = (*self.items).clone();
        self.form.mark_excluded(&mut items);
        if !needed.is_empty() {
            self.fill_fields(&needed, &mut items);
        }
        let listing = Listing {
            existing: &self.existing,
            fold: self.fold,
        };
        Preview::compute_form(&self.form, &items, listing, self.generation)
    }

    fn fill_fields(&self, needed: &BTreeSet<String>, items: &mut [Item]) {
        let Ok(mut cache) = self.fields.lock() else {
            return;
        };
        if !needed.is_subset(&cache.groups) || cache.values.len() != items.len() {
            let groups: BTreeSet<String> = cache.groups.union(needed).cloned().collect();
            cache.values = items
                .iter()
                .map(|item| read_fields(&self.registry, &groups, item))
                .collect();
            cache.groups = groups;
        }
        for (item, values) in items.iter_mut().zip(&cache.values) {
            item.fields = values.clone();
        }
    }
}

fn read_fields(registry: &FieldRegistry, groups: &BTreeSet<String>, item: &Item) -> ItemFields {
    item.parent
        .join(&ProviderPath::from_text(&item.name))
        .ok()
        .and_then(|location| rusty_fs::os_path(&location).ok())
        .map(|path| registry.read(groups, &path))
        .unwrap_or_default()
}
