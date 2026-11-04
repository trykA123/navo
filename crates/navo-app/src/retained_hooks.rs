use std::sync::Arc;

use navo_core::Location;
use navo_core::retention::{RetainedState as Stored, RetentionEntry, RetentionId};
use navo_fs::{CancelToken, ProviderOps};
use navo_retention::{Error, RestoreRequest, RetentionRun, Vault};
use navo_store::{RetentionCatalog, RetentionStoreError, StoreDirs};
use navo_ui::browser::{NoRetained, RetainedHooks, RetainedKind, RetainedRow, RetainedState};

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
            .map_err(|error| list_failed(&error, "Database"))?;
        RetentionCatalog::new(db)
            .and_then(|catalog| catalog.live_after(after.map_or(0, |id| id.0), u32::from(limit)))
            .map_err(|error| list_failed(&error, "Catalog"))?
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
        let restored = vault
            .restore(
                request,
                &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
            )
            .map_err(|error| failed("retained-restore-failed", &entry, &error))?;
        tracing::info!(id = id.0, "retained entry restored");
        // An interrupted restore that had already landed reports where it landed.
        Ok(restored.target.map_or(target, |landed| landed.location))
    }

    fn purge(&self, id: RetentionId) -> Result<(), String> {
        let _lease = crate::jobs::claim_exclusive(&self.dirs.data)?;
        let mut vault = self.vault()?;
        let mut entry = vault.get(id).map_err(|error| refused(&error))?;
        if entry.pinned && !entry.kept {
            return Err(navo_l10n::tr("retained-busy"));
        }
        if entry.state == Stored::Preparing {
            entry = settle(&mut vault, &entry)?;
        }
        if unrecorded_conflict(&entry) {
            return Err(navo_l10n::tr("retained-stuck"));
        }
        if entry.state == Stored::Aborted {
            return Err(navo_l10n::tr("retained-missing"));
        }
        if !matches!(
            entry.state,
            Stored::Ready | Stored::Purging | Stored::Restoring | Stored::Conflict
        ) {
            return Err(navo_l10n::tr("retained-not-deletable"));
        }
        let purged = vault
            .delete(id, &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}))
            .map_err(|error| failed("retained-purge-failed", &entry, &error))?;
        if let Some(landed) = purged.target.filter(|_| purged.state == Stored::Restored) {
            tracing::info!(id = id.0, "retained entry was already restored");
            let folder = landed.location.parent().unwrap_or(landed.location);
            return Err(navo_l10n::t!(
                "retained-already-restored",
                name = names::shown(&entry),
                folder = folder.as_str()
            ));
        }
        tracing::info!(id = id.0, "retained entry purged");
        Ok(())
    }
}

impl VaultHooks {
    fn vault(&self) -> Result<Vault<'_>, String> {
        let (db, _) = crate::crash::open_db(&self.dirs.journal_file())
            .map_err(|error| navo_l10n::t!("retained-open-failed", error = error.to_string()))?;
        Vault::new(db, &self.dirs.data, self.ops.as_ref())
            .map_err(|error| navo_l10n::t!("retained-open-failed", error = error.to_string()))
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
        bytes: entry
            .record
            .bytes()
            .map_err(|error| list_failed(&error, "Invalid"))?,
        deleted_at_ms: entry.record.created_at,
        expires_at_ms: entry.record.expires_at,
        state: state(entry),
    })
}

/// Settles an interrupted capture so the user's delete sees where its files are.
fn settle(vault: &mut Vault<'_>, entry: &RetentionEntry) -> Result<RetentionEntry, String> {
    let cancel = CancelToken::new();
    let mut progress = |_| {};
    let mut run = RetentionRun::new(&cancel, &mut progress);
    vault.reconcile(entry.id, &mut run).map_err(|error| {
        if vault.needs_attention(&error, entry) {
            tracing::warn!(
                id = entry.id.0,
                error_kind = error.kind(),
                "retained entry cannot settle"
            );
            navo_l10n::tr("retained-stuck")
        } else {
            failed("retained-purge-failed", entry, &error)
        }
    })
}

/// A Conflict whose holding folder was never recorded: the catalog refuses to purge it.
fn unrecorded_conflict(entry: &RetentionEntry) -> bool {
    entry.state == Stored::Conflict && entry.record.container_identity.is_none()
}

fn state(entry: &RetentionEntry) -> RetainedState {
    match entry.state {
        _ if entry.pinned && !entry.kept => RetainedState::Busy,
        _ if unrecorded_conflict(entry) => RetainedState::Stuck,
        // The action settles an interrupted restore before it runs.
        Stored::Ready | Stored::Restoring => RetainedState::Ready,
        _ => RetainedState::NeedsAttention,
    }
}

fn list_failed(error: &dyn std::fmt::Display, error_kind: &str) -> String {
    tracing::warn!(error_kind, "listing retained entries failed");
    navo_l10n::t!("retained-list-failed", error = error.to_string())
}

fn refused(error: &Error) -> String {
    match error {
        Error::Pinned(_) | Error::Catalog(RetentionStoreError::Pinned(_)) => {
            navo_l10n::tr("retained-busy")
        }
        Error::State(_) => navo_l10n::tr("retained-not-ready"),
        Error::Catalog(RetentionStoreError::Missing(_)) => navo_l10n::tr("retained-missing"),
        other => navo_l10n::t!("retained-open-failed", error = other.to_string()),
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
            tracing::warn!(
                id = entry.id.0,
                error_kind = other.kind(),
                "retained entry action failed"
            );
            navo_l10n::t!(id, name = names::shown(entry), error = other.to_string())
        }
    }
}

#[cfg(test)]
mod tests;
