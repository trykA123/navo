use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::Arc;

use navo_core::{AncestorMemo, Location, OpKind, OperationPlan, ProviderPath, Resolution, Step};
use navo_fs::ProviderOps;
use navo_store::{RetentionCatalog, RetentionSettings, SettingsError, StoreDirs};
use navo_ui::browser::{LogFactory, OpLog, WriteFactory};

use super::mutation_lock::{self, LeaseError, Mode, Wait};
use super::{
    JournalLog, SessionHolds, SharedJournal, create_log, default_lock, deletion, overwrite,
};

#[cfg(test)]
pub fn profile_factory(journal: SharedJournal, dirs: StoreDirs) -> LogFactory {
    factory(journal, dirs, None, None)
}

#[cfg(test)]
pub fn retaining_factory(
    journal: SharedJournal,
    dirs: StoreDirs,
    drive: Arc<dyn ProviderOps>,
) -> LogFactory {
    factory(journal, dirs, Some(Drive::Probed(drive)), None)
}

#[cfg(test)]
pub fn experimental_factory(journal: SharedJournal, dirs: StoreDirs) -> LogFactory {
    factory(journal, dirs, Some(Drive::Unprobed(None)), None)
}

#[cfg(test)]
pub fn confined_factory(journal: SharedJournal, dirs: StoreDirs, ceiling: Location) -> LogFactory {
    factory(journal, dirs, Some(Drive::Unprobed(Some(ceiling))), None)
}

#[derive(Clone)]
pub(super) enum Drive {
    Probed(Arc<dyn ProviderOps>),
    #[cfg(test)]
    Unprobed(Option<Location>),
}

pub(super) fn factory(
    journal: SharedJournal,
    dirs: StoreDirs,
    drive: Option<Drive>,
    session: Option<SessionHolds>,
) -> LogFactory {
    let experimental = drive.is_some();
    #[cfg(test)]
    let ceiling = match &drive {
        Some(Drive::Unprobed(ceiling)) => ceiling.clone(),
        _ => None,
    };
    Arc::new(move |plan| {
        guard(plan, &dirs, experimental)?;
        let settings = if experimental && retains(plan.kind) {
            retention_for(plan, &dirs)?
        } else {
            RetentionSettings::off()
        };
        let settings =
            if settings.maintenance_policy().is_some() && !drive_retains(drive.as_ref(), plan) {
                RetentionSettings::off()
            } else {
                settings
            };
        refuse_retained_save(plan, &settings)?;
        let retained = delete_job(plan, &settings, &dirs)?;
        #[cfg(test)]
        let retained = retained.map(|job| deletion::RetainedJob {
            ceiling: ceiling.clone(),
            ..job
        });
        let mut log = if matches!(plan.kind, OpKind::Copy | OpKind::Move)
            && let Some(policy) = settings.overwrite_policy()
        {
            let plan = navo_core::retention::RetainedWritePlan::new(plan.clone(), policy)
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
        log.session = session.clone().filter(|holds| holds.marks(plan));
        log.experimental_retention = experimental;
        log.delete_policy = settings
            .delete_policy()
            .filter(|_| plan.kind == OpKind::Delete);
        Ok(Box::new(log) as Box<dyn OpLog>)
    })
}

pub(super) fn check_confirmed(
    log: &JournalLog,
    operation: &navo_ui::browser::LoggedOperation<'_>,
) -> Result<(), String> {
    if !log.experimental_retention || log.undoing || operation.plan.kind != OpKind::Delete {
        return Ok(());
    }
    let confirmed = operation.retain_deletes;
    let confirmed_retains = confirmed.is_some_and(|policy| {
        !navo_core::retention::delete_exceeds_capacity(operation.plan, policy)
    });
    if log.delete_policy != confirmed || log.retained.is_some() != confirmed_retains {
        return Err(navo_l10n::tr("job-retention-changed"));
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
    match navo_core::retention::RetainedDeletePlan::new(plan.clone(), policy) {
        Ok(plan) => Ok(Some(deletion::RetainedJob {
            data: dirs.data.clone(),
            plan,
            #[cfg(test)]
            ceiling: None,
        })),
        Err(navo_core::retention::RetentionError::DeleteTooLarge) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn drive_retains(drive: Option<&Drive>, plan: &OperationPlan) -> bool {
    let ops = match drive {
        Some(Drive::Probed(ops)) => ops,
        #[cfg(test)]
        Some(Drive::Unprobed(_)) => return true,
        None => return false,
    };
    let probe = match plan.kind {
        OpKind::Delete => plan.steps.iter().find_map(|step| match step {
            Step::RemoveFile { location }
            | Step::RemoveLink { location }
            | Step::RemoveFolder { location } => Some(location),
            _ => None,
        }),
        OpKind::Copy | OpKind::Move | OpKind::Save => {
            let Some(target) = overwritten_target(plan) else {
                return true;
            };
            Some(target)
        }
        _ => return true,
    };
    probe.is_some_and(|at| navo_retention::can_retain(ops.as_ref(), at))
}

fn refuse_retained_save(plan: &OperationPlan, settings: &RetentionSettings) -> Result<(), String> {
    if plan.kind == OpKind::Save && settings.overwrite_policy().is_some() {
        return Err("saving with overwrite retention is not supported yet".to_owned());
    }
    Ok(())
}

fn retains(kind: OpKind) -> bool {
    matches!(
        kind,
        OpKind::Copy | OpKind::Move | OpKind::Delete | OpKind::Save
    )
}

fn overwritten_target(plan: &OperationPlan) -> Option<&Location> {
    plan.conflicts
        .iter()
        .find(|conflict| conflict.resolution == Resolution::Overwrite)
        .map(|conflict| &conflict.target)
        .or_else(|| {
            plan.steps.iter().find_map(|step| match step {
                Step::WriteFile { target, .. } => Some(target),
                _ => None,
            })
        })
}

#[derive(Debug)]
pub(crate) struct WritePermit {
    _guard: WriteGuard,
}

#[derive(Debug)]
enum WriteGuard {
    Experimental { _lease: File },
    Default { _hold: default_lock::Hold },
}

pub(crate) fn claim_write(
    dirs: &StoreDirs,
    target: &Location,
    experimental: bool,
) -> Result<Option<WritePermit>, String> {
    write_lease(dirs, target, experimental, Wait::bounded()).map_err(String::from)
}

fn write_lease(
    dirs: &StoreDirs,
    target: &Location,
    experimental: bool,
    wait: Wait<'_>,
) -> Result<Option<WritePermit>, LeaseError> {
    let permit = if experimental {
        WriteGuard::Experimental {
            _lease: mutation_lock::lease(&dirs.data, Mode::Shared, wait)?,
        }
    } else {
        WriteGuard::Default {
            _hold: default_lock::acquire(&dirs.data, Mode::Shared, wait)?,
        }
    };
    let resolved =
        navo_fs::mutation_location(target).map_err(|error| LeaseError::Failed(error.message()))?;
    guard_paths(&[resolved], dirs, experimental).map_err(LeaseError::Failed)?;
    Ok(Some(WritePermit { _guard: permit }))
}

pub(crate) fn write_factory(dirs: StoreDirs, experimental: bool) -> WriteFactory {
    Arc::new(move |target| {
        write_lease(&dirs, target, experimental, Wait::bounded())
            .map(|lease| Box::new(lease) as Box<dyn Send>)
            .map_err(|error| {
                let kind = error.kind();
                tracing::warn!(error = %String::from(error), "file creation refused");
                navo_core::FsError {
                    location: Some(target.clone()),
                    ..navo_core::FsError::new(kind)
                }
            })
    })
}

pub(crate) fn retains_deletes(dirs: &StoreDirs) -> Option<navo_core::retention::RetentionPolicy> {
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
        if plan.kind == OpKind::Delete || overwritten_target(plan).is_some() {
            Err(error)
        } else {
            Ok(RetentionSettings::off())
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
            return Err(navo_l10n::t!(
                "job-settings-read",
                file = file,
                error = error.to_string()
            ));
        }
    };
    let (settings, unknown) =
        navo_store::parse_retention_settings(&text).map_err(|error| match error {
            SettingsError::Invalid { line, message, .. } => {
                navo_l10n::t!(
                    "job-settings-invalid",
                    file = file.clone(),
                    line = line,
                    error = message
                )
            }
            other => navo_l10n::t!(
                "job-settings-read",
                file = file.clone(),
                error = other.to_string()
            ),
        })?;
    if unknown.iter().any(|key| key.starts_with("retention.")) {
        return Err(navo_l10n::t!("job-settings-unknown", file = file));
    }
    Ok(settings)
}

pub(super) fn guard(
    plan: &OperationPlan,
    dirs: &StoreDirs,
    experimental: bool,
) -> Result<(), String> {
    guard_paths(resolve(plan)?.paths(), dirs, experimental)
}

pub(super) fn check_plan(
    log: &JournalLog,
    operation: &navo_ui::browser::LoggedOperation<'_>,
) -> Result<(), String> {
    if log.profile.is_none() && !log.experimental_retention {
        return Ok(());
    }
    let resolved = resolve(operation.plan)?;
    if let Some(dirs) = &log.profile {
        guard_paths(resolved.paths(), dirs, log.experimental_retention)?;
    }
    if log.experimental_retention {
        navo_fs::check_resolved_mutation(&resolved, operation.ops, operation.cancel)
    } else {
        navo_fs::check_relocation(operation.plan, operation.ops, operation.cancel)
    }
    .map_err(|error| error.message())
}

fn resolve(plan: &OperationPlan) -> Result<navo_fs::ResolvedPlan<'_>, String> {
    navo_fs::ResolvedPlan::new(plan).map_err(|error| error.message())
}

pub(super) fn mutated_paths(plan: &OperationPlan) -> Result<Vec<Location>, String> {
    resolve(plan).map(navo_fs::ResolvedPlan::into_paths)
}

fn canonical_ancestor(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let existing = path.ancestors().find(|ancestor| ancestor.exists())?;
    let rest = path.strip_prefix(existing).ok()?;
    Some(std::fs::canonicalize(existing).ok()?.join(rest))
}

fn guard_paths(paths: &[Location], dirs: &StoreDirs, experimental: bool) -> Result<(), String> {
    let mut retained = AncestorMemo::new(retained_level);
    if paths.iter().any(|path| retained.any(path)) {
        return Err(navo_l10n::tr("job-retained-protected"));
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
        if touches_any(paths, &[Some(location), resolved]) {
            return Err(navo_l10n::tr("job-profile-protected"));
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
        return Err(navo_l10n::tr("job-retained-parent-protected"));
    }
    Ok(())
}

fn retained_level(_: &Location, name: Option<&ProviderPath>) -> bool {
    name.is_some_and(|name| navo_core::brand::is_retained_container(&name.to_text()))
}

fn touches_any(paths: &[Location], targets: &[Option<Location>]) -> bool {
    let targets: Vec<_> = targets.iter().flatten().collect();
    let below: Vec<_> = targets
        .iter()
        .flat_map(|target| std::iter::successors(Some((*target).clone()), Location::parent))
        .collect();
    let mut above = AncestorMemo::new(|level: &Location, _: Option<&ProviderPath>| {
        targets.iter().any(|target| level.same_place(target))
    });
    paths
        .iter()
        .any(|path| above.any(path) || below.iter().any(|level| level.same_place(path)))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod timing_tests;
