use std::collections::HashMap;

use rusty_core::Location;
use rusty_core::retention::{RetentionEntry, RetentionId, RetentionPolicy};
use rusty_fs::ProviderOps;
use rusty_store::{RetentionCatalog, StoreDirs};

use crate::jobs::{SharedJournal, lock};

#[derive(Default)]
pub(super) struct Live {
    pub(super) volumes: HashMap<u64, Location>,
    pub(super) sizes: HashMap<RetentionId, u64>,
    pub(super) totals: HashMap<u64, u128>,
}

impl Live {
    pub(super) fn retain_pressured(
        &mut self,
        ops: &dyn ProviderOps,
        policy: RetentionPolicy,
    ) -> Result<(), String> {
        let mut pressured = HashMap::new();
        for (volume, at) in &self.volumes {
            let free = ops.available_space(at).map_err(|error| error.message())?;
            if self.totals.get(volume).copied().unwrap_or(0) > u128::from(policy.max_volume_bytes())
                || free.is_some_and(|free| free < policy.min_free_bytes())
            {
                pressured.insert(*volume, at.clone());
            }
        }
        self.volumes = pressured;
        Ok(())
    }
}

pub(super) enum Catalog<'a> {
    Shared(&'a SharedJournal),
    Dedicated(RetentionCatalog),
}

impl<'a> Catalog<'a> {
    pub(super) fn open(
        dirs: &StoreDirs,
        shared: Option<&'a SharedJournal>,
    ) -> Result<Self, String> {
        if let Some(shared) = shared {
            return Ok(Self::Shared(shared));
        }
        let (db, _) =
            crate::crash::open_db(&dirs.journal_file()).map_err(|error| error.to_string())?;
        RetentionCatalog::new(db)
            .map(Self::Dedicated)
            .map_err(|error| error.to_string())
    }

    pub(super) fn page(&self, after: i64) -> Result<Vec<RetentionEntry>, String> {
        match self {
            Self::Shared(shared) => {
                RetentionCatalog::live_in(lock(shared).database(), after, super::PAGE)
            }
            Self::Dedicated(catalog) => catalog.live_after(after, super::PAGE),
        }
        .map_err(|error| error.to_string())
    }
}
