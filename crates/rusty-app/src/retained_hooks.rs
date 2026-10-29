use std::sync::Arc;

use rusty_core::Location;
use rusty_core::retention::{RetainedState as Stored, RetentionEntry, RetentionId};
use rusty_fs::{CancelToken, ProviderOps};
use rusty_retention::{Error, RestoreRequest, RetentionRun, Vault};
use rusty_store::{RetentionCatalog, RetentionStoreError, StoreDirs};
use rusty_ui::browser::{NoRetained, RetainedHooks, RetainedKind, RetainedRow, RetainedState};

mod names;

const MAX_PAGE: u16 = 1000;

pub fn hooks(
    dirs: Option<&StoreDirs>,
    retention: bool,
    ops: Arc<dyn ProviderOps>,
) -> Arc<dyn RetainedHooks> {
    match dirs {
        Some(dirs) if retention => Arc::new(VaultHooks {
            dirs: dirs.clone(),
            ops,
        }),
        _ => Arc::new(NoRetained),
    }
}

pub struct VaultHooks {
    dirs: StoreDirs,
    ops: Arc<dyn ProviderOps>,
}

impl RetainedHooks for VaultHooks {
    fn list(&self, after: Option<RetentionId>, limit: u16) -> Result<Vec<RetainedRow>, String> {
        let limit = limit.min(MAX_PAGE);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let (db, _) = crate::crash::open_db(&self.dirs.journal_file())
            .map_err(|error| list_failed(&error))?;
        RetentionCatalog::new(db)
            .and_then(|catalog| catalog.live_after(after.map_or(0, |id| id.0), u32::from(limit)))
            .map_err(|error| list_failed(&error))?
            .iter()
            .map(row)
            .collect()
    }

    fn restore(&self, id: RetentionId) -> Result<Location, String> {
        let _lease = crate::jobs::claim_exclusive(&self.dirs.data)?;
        let mut vault = self.vault()?;
        let entry = vault.get(id).map_err(|error| refused(&error))?;
        let target = names::free_target(self.ops.as_ref(), &entry)?;
        let request = &RestoreRequest {
            id,
            target: &target,
        };
        vault
            .restore(
                request,
                &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
            )
            .map_err(|error| failed("retained-restore-failed", &entry, &error))?;
        tracing::info!(id = id.0, "retained entry restored");
        Ok(target)
    }

    fn purge(&self, id: RetentionId) -> Result<(), String> {
        let _lease = crate::jobs::claim_exclusive(&self.dirs.data)?;
        let mut vault = self.vault()?;
        let entry = vault.get(id).map_err(|error| refused(&error))?;
        if entry.pinned {
            return Err(rusty_l10n::tr("retained-busy"));
        }
        if !matches!(entry.state, Stored::Ready | Stored::Purging) {
            return Err(rusty_l10n::tr("retained-not-ready"));
        }
        vault
            .purge(id, &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}))
            .map_err(|error| failed("retained-purge-failed", &entry, &error))?;
        tracing::info!(id = id.0, "retained entry purged");
        Ok(())
    }
}

impl VaultHooks {
    fn vault(&self) -> Result<Vault<'_>, String> {
        let (db, _) = crate::crash::open_db(&self.dirs.journal_file())
            .map_err(|error| rusty_l10n::t!("retained-open-failed", error = error.to_string()))?;
        Vault::new(db, &self.dirs.data, self.ops.as_ref())
            .map_err(|error| rusty_l10n::t!("retained-open-failed", error = error.to_string()))
    }
}

fn row(entry: &RetentionEntry) -> Result<RetainedRow, String> {
    Ok(RetainedRow {
        id: entry.id,
        original: entry.record.source.clone(),
        kind: if entry.record.payload_name.is_some() {
            RetainedKind::Overwrite
        } else {
            RetainedKind::Delete
        },
        bytes: entry.record.bytes().map_err(|error| list_failed(&error))?,
        deleted_at_ms: entry.record.created_at,
        expires_at_ms: entry.record.expires_at,
        state: state(entry),
    })
}

fn state(entry: &RetentionEntry) -> RetainedState {
    match entry.state {
        _ if entry.pinned => RetainedState::Busy,
        Stored::Ready => RetainedState::Ready,
        _ => RetainedState::NeedsAttention,
    }
}

fn list_failed(error: &dyn std::fmt::Display) -> String {
    tracing::warn!(%error, "listing retained entries failed");
    rusty_l10n::t!("retained-list-failed", error = error.to_string())
}

fn refused(error: &Error) -> String {
    match error {
        Error::Pinned(_) | Error::Catalog(RetentionStoreError::Pinned(_)) => {
            rusty_l10n::tr("retained-busy")
        }
        Error::State(_) => rusty_l10n::tr("retained-not-ready"),
        Error::Catalog(RetentionStoreError::Missing(_)) => rusty_l10n::tr("retained-missing"),
        other => rusty_l10n::t!("retained-open-failed", error = other.to_string()),
    }
}

fn failed(id: &str, entry: &RetentionEntry, error: &Error) -> String {
    match error {
        Error::Pinned(_)
        | Error::State(_)
        | Error::Catalog(RetentionStoreError::Pinned(_) | RetentionStoreError::Missing(_)) => {
            refused(error)
        }
        other => {
            tracing::warn!(id = entry.id.0, error = %other, "retained entry action failed");
            rusty_l10n::t!(id, name = names::shown(entry), error = other.to_string())
        }
    }
}

#[cfg(test)]
mod tests;
