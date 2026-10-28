use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::Arc;

use rusty_core::{Location, OpKind, OperationPlan, Resolution, Step};
use rusty_store::{RetentionCatalog, RetentionSettings, SettingsError, StoreDirs};
use rusty_ui::browser::{LogFactory, OpLog, WriteFactory};

use super::mutation_lock::{self, LeaseError, Mode, Wait};
use super::{JournalLog, SharedJournal, create_log, deletion, overwrite};

pub fn profile_factory(journal: SharedJournal, dirs: StoreDirs) -> LogFactory {
    factory(journal, dirs, false)
}

pub fn experimental_factory(journal: SharedJournal, dirs: StoreDirs) -> LogFactory {
    factory(journal, dirs, true)
}

fn factory(journal: SharedJournal, dirs: StoreDirs, experimental: bool) -> LogFactory {
    Arc::new(move |plan| {
        guard(plan, &dirs, experimental)?;
        let settings =
            if experimental && matches!(plan.kind, OpKind::Copy | OpKind::Move | OpKind::Delete) {
                retention_for(plan, &dirs)?
            } else {
                RetentionSettings::default()
            };
        let retained = delete_job(plan, &settings, &dirs)?;
        let mut log = if matches!(plan.kind, OpKind::Copy | OpKind::Move)
            && let Some(policy) = settings.overwrite_policy()
        {
            let plan = rusty_core::retention::RetainedWritePlan::new(plan.clone(), policy)
                .map_err(|error| error.to_string())?;
            overwrite::create(
                &journal,
                overwrite::WriteJob {
                    data: dirs.data.clone(),
                    plan,
                },
            )?
        } else {
            create_log(&journal, plan, retained)?
        };
        log.profile = Some(dirs.clone());
        log.experimental_retention = experimental;
        log.delete_policy = settings
            .delete_policy()
            .filter(|_| plan.kind == OpKind::Delete);
        Ok(Box::new(log) as Box<dyn OpLog>)
    })
}

pub(super) fn check_confirmed(
    log: &JournalLog,
    operation: &rusty_ui::browser::LoggedOperation<'_>,
) -> Result<(), String> {
    if !log.experimental_retention || log.undoing || operation.plan.kind != OpKind::Delete {
        return Ok(());
    }
    let confirmed = operation.retain_deletes;
    let confirmed_retains = confirmed.is_some_and(|policy| {
        !rusty_core::retention::delete_exceeds_capacity(operation.plan, policy)
    });
    if log.delete_policy != confirmed || log.retained.is_some() != confirmed_retains {
        return Err(rusty_l10n::tr("job-retention-changed"));
    }
    Ok(())
}

fn delete_job(
    plan: &OperationPlan,
    settings: &RetentionSettings,
    dirs: &StoreDirs,
) -> Result<Option<deletion::RetainedJob>, String> {
    if plan.kind != OpKind::Delete {
        return Ok(None);
    }
    let Some(policy) = settings.delete_policy() else {
        return Ok(None);
    };
    match rusty_core::retention::RetainedDeletePlan::new(plan.clone(), policy) {
        Ok(plan) => Ok(Some(deletion::RetainedJob {
            data: dirs.data.clone(),
            plan,
        })),
        Err(rusty_core::retention::RetentionError::DeleteTooLarge) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn claim_write(
    dirs: &StoreDirs,
    target: &Location,
    experimental: bool,
) -> Result<Option<File>, String> {
    write_lease(dirs, target, experimental, Wait::bounded()).map_err(String::from)
}

fn write_lease(
    dirs: &StoreDirs,
    target: &Location,
    experimental: bool,
    wait: Wait<'_>,
) -> Result<Option<File>, LeaseError> {
    let lease = experimental
        .then(|| mutation_lock::lease(&dirs.data, Mode::Shared, wait))
        .transpose()?;
    let resolved =
        rusty_fs::mutation_location(target).map_err(|error| LeaseError::Failed(error.message()))?;
    guard_paths(&[resolved], dirs, experimental).map_err(LeaseError::Failed)?;
    Ok(lease)
}

pub(crate) fn write_factory(dirs: StoreDirs, experimental: bool) -> WriteFactory {
    Arc::new(move |target| {
        write_lease(&dirs, target, experimental, Wait::bounded())
            .map(|lease| Box::new(lease) as Box<dyn Send>)
            .map_err(|error| {
                let kind = error.kind();
                tracing::warn!(error = %String::from(error), "file creation refused");
                rusty_core::FsError {
                    location: Some(target.clone()),
                    ..rusty_core::FsError::new(kind)
                }
            })
    })
}

pub(crate) fn retains_deletes(dirs: &StoreDirs) -> Option<rusty_core::retention::RetentionPolicy> {
    match settings(dirs) {
        Ok(settings) => settings.delete_policy(),
        Err(error) => {
            tracing::warn!(%error, "delete confirmation cannot read retention settings");
            None
        }
    }
}

fn retention_for(plan: &OperationPlan, dirs: &StoreDirs) -> Result<RetentionSettings, String> {
    settings(dirs).or_else(|error| {
        let overwrites = plan
            .conflicts
            .iter()
            .any(|conflict| conflict.resolution == Resolution::Overwrite);
        if plan.kind == OpKind::Delete || overwrites {
            Err(error)
        } else {
            Ok(RetentionSettings::default())
        }
    })
}

pub(super) fn settings(dirs: &StoreDirs) -> Result<RetentionSettings, String> {
    read_retention(&dirs.settings_file())
}

pub(crate) fn read_retention(path: &Path) -> Result<RetentionSettings, String> {
    let file = path.display().to_string();
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(RetentionSettings::default());
        }
        Err(error) => {
            return Err(rusty_l10n::t!(
                "job-settings-read",
                file = file,
                error = error.to_string()
            ));
        }
    };
    let (settings, unknown) =
        rusty_store::parse_retention_settings(&text).map_err(|error| match error {
            SettingsError::Invalid { line, message, .. } => {
                rusty_l10n::t!(
                    "job-settings-invalid",
                    file = file.clone(),
                    line = line,
                    error = message
                )
            }
            other => rusty_l10n::t!(
                "job-settings-read",
                file = file.clone(),
                error = other.to_string()
            ),
        })?;
    if unknown.iter().any(|key| key.starts_with("retention.")) {
        return Err(rusty_l10n::t!("job-settings-unknown", file = file));
    }
    Ok(settings)
}

pub(super) fn guard(
    plan: &OperationPlan,
    dirs: &StoreDirs,
    experimental: bool,
) -> Result<(), String> {
    guard_paths(&mutated_paths(plan)?, dirs, experimental)
}

pub(super) fn mutated_paths(plan: &OperationPlan) -> Result<Vec<Location>, String> {
    plan.steps
        .iter()
        .flat_map(mutated)
        .map(rusty_fs::mutation_location)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.message())
}

fn canonical_ancestor(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let existing = path.ancestors().find(|ancestor| ancestor.exists())?;
    let rest = path.strip_prefix(existing).ok()?;
    Some(std::fs::canonicalize(existing).ok()?.join(rest))
}

fn guard_paths(paths: &[Location], dirs: &StoreDirs, experimental: bool) -> Result<(), String> {
    if paths.iter().any(reserved) {
        return Err(rusty_l10n::tr("job-retained-protected"));
    }
    for path in [&dirs.config, &dirs.data] {
        let location = crate::startup::path_location(path).ok_or("invalid profile path")?;
        let resolved = match std::fs::canonicalize(path) {
            Ok(resolved) => {
                Some(crate::startup::path_location(&resolved).ok_or("invalid profile path")?)
            }
            Err(_) if !experimental => canonical_ancestor(path)
                .and_then(|resolved| crate::startup::path_location(&resolved)),
            Err(error) => return Err(error.to_string()),
        };
        if paths.iter().any(|path| {
            intersects(path, &location)
                || resolved
                    .as_ref()
                    .is_some_and(|resolved| intersects(path, resolved))
        }) {
            return Err(rusty_l10n::tr("job-profile-protected"));
        }
    }
    if !experimental {
        return Ok(());
    }
    let (db, _) = crate::crash::open_db(&dirs.journal_file()).map_err(|error| error.to_string())?;
    let catalog = RetentionCatalog::new(db).map_err(|error| error.to_string())?;
    if !catalog.has_live().map_err(|error| error.to_string())? {
        return Ok(());
    }
    if catalog
        .intersects_live(paths)
        .map_err(|error| error.to_string())?
    {
        return Err(rusty_l10n::tr("job-retained-parent-protected"));
    }
    Ok(())
}

fn mutated(step: &Step) -> Vec<&Location> {
    match step {
        Step::Rename { source, target } => vec![source, target],
        Step::CopyFile { target, .. } | Step::MakeFolder { target } => vec![target],
        Step::MakeLink { at, .. } => vec![at],
        Step::Recycle { location }
        | Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => vec![location],
    }
}

fn reserved(path: &Location) -> bool {
    std::iter::successors(Some(path.clone()), Location::parent)
        .filter_map(|path| path.name())
        .any(|name| rusty_core::brand::is_retained_container(&name.to_text()))
}

fn intersects(a: &Location, b: &Location) -> bool {
    std::iter::successors(Some(a.clone()), Location::parent).any(|path| path.same_place(b))
        || std::iter::successors(Some(b.clone()), Location::parent).any(|path| path.same_place(a))
}

#[cfg(test)]
mod tests;
