use std::collections::HashMap;

use rusty_core::retention::{RetainedKind, RetentionRecord};
use rusty_core::{FileIdentity, Location, ProviderPath};
use rusty_fs::ProviderOps;

use crate::{Error, proof};

pub(super) struct Ancestors {
    root: Location,
    folders: HashMap<Vec<String>, FileIdentity>,
    scope: proof::Scope,
}

impl Ancestors {
    pub fn new(root: &Location, scope: proof::Scope) -> Self {
        Self {
            root: root.clone(),
            folders: HashMap::new(),
            scope,
        }
    }

    pub fn recorded(record: &RetentionRecord, scope: proof::Scope) -> Result<Self, Error> {
        let mut guard = Self::new(&record.held()?, scope);
        for node in &record.nodes {
            if node.kind == RetainedKind::Folder {
                guard.remember(&node.path, node.identity);
            }
        }
        Ok(guard)
    }

    pub fn remember(&mut self, path: &[String], identity: FileIdentity) {
        self.folders.insert(path.to_vec(), identity);
    }

    pub fn verify(&self, ops: &dyn ProviderOps, path: &[String]) -> Result<bool, Error> {
        let mut location = self.root.clone();
        for length in 0..path.len() {
            let expected = self
                .folders
                .get(&path[..length])
                .ok_or_else(|| Error::Changed(location.clone()))?;
            if proof::present(ops, &location)?.is_none() {
                return Ok(false);
            }
            proof::folder(ops, &location, *expected)?;
            proof::on_scope(ops, &location, self.scope)?;
            location = location
                .join(&ProviderPath::from_text(&path[length]))
                .map_err(|_| Error::Changed(location.clone()))?;
        }
        Ok(true)
    }
}
