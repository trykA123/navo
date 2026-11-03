use std::sync::Arc;

use navo_core::Location;
use navo_fs::ProviderOps;
use navo_store::StoreDirs;
use navo_ui::browser::Operations;

use crate::jobs::maintenance::MaintenanceTimer;
use crate::{files, jobs, metrics, rename_hooks, retained_hooks};

pub(super) struct ProfileServices {
    pub(super) memory: Option<files::SharedDb>,
    pub(super) files: navo_ui::window_view::FileHooks,
    pub(super) operations: Operations,
    pub(super) rename: navo_ui::browser::RenameHooks,
    pub(super) retained: Arc<dyn navo_ui::browser::RetainedHooks>,
    pub(super) sessions: Option<jobs::SessionHolds>,
    pub(super) maintenance: MaintenanceTimer,
}

pub(super) fn profile_services(
    (dirs, journal): (Option<&StoreDirs>, Option<jobs::SharedJournal>),
    home: &Location,
    experimental: bool,
    local: &Arc<dyn ProviderOps>,
) -> ProfileServices {
    let (operations, sessions) = operations((dirs, journal), home, experimental, local);
    let journal = sessions.as_ref().map(jobs::SessionHolds::journal);
    let memory = journal.clone();
    let files = files::profile_hooks(memory.clone(), dirs, experimental);
    let rename = rename_hooks::profile_hooks(journal, memory.clone(), dirs, experimental);
    metrics::mark("databases opened");
    ProfileServices {
        memory: memory.clone(),
        files,
        operations,
        rename,
        retained: retained_hooks::hooks(dirs, experimental, local.clone()),
        sessions,
        maintenance: MaintenanceTimer::start_with_memory(
            (dirs, memory),
            experimental,
            local.clone(),
        ),
    }
}

pub(super) fn operations(
    (dirs, journal): (Option<&StoreDirs>, Option<jobs::SharedJournal>),
    home: &Location,
    experimental: bool,
    local: &Arc<dyn ProviderOps>,
) -> (Operations, Option<jobs::SessionHolds>) {
    let mut protected = vec![home.clone()];
    let Some(dirs) = dirs else {
        let unavailable =
            unavailable_operations(protected, None, "job-profile-unavailable", experimental);
        return (unavailable, None);
    };
    protected.extend(
        [&dirs.config, &dirs.data]
            .into_iter()
            .filter_map(|path| Location::new(path.to_str()?).ok()),
    );
    let db = dirs.journal_file();
    let journal = journal.or_else(|| {
        dirs.ensure().ok().and_then(|()| {
            jobs::open(&db)
                .inspect_err(|error| tracing::error!(%error, "opening the job journal failed"))
                .ok()
        })
    });
    let Some(journal) = journal else {
        let unavailable = unavailable_operations(
            protected,
            Some(dirs),
            "job-journal-unavailable",
            experimental,
        );
        return (unavailable, None);
    };
    let retention_dirs = dirs.clone();
    let sessions = jobs::SessionHolds::new(journal, dirs.clone());
    let logged = Operations {
        retain_deletes: experimental.then(|| {
            Arc::new(move || jobs::retains_deletes(&retention_dirs))
                as Arc<dyn Fn() -> Option<navo_core::retention::RetentionPolicy> + Send + Sync>
        }),
        write: Some(jobs::write_factory(dirs.clone(), experimental)),
        log: sessions.factory(experimental.then(|| local.clone())),
        log_without_retention: experimental.then(|| sessions.factory_without_retention()),
        protected,
    };
    (logged, Some(sessions))
}

fn unavailable_operations(
    protected: Vec<Location>,
    dirs: Option<&StoreDirs>,
    message: &'static str,
    experimental: bool,
) -> Operations {
    if !experimental {
        return Operations {
            protected,
            log: jobs::guard_factory(dirs.cloned()),
            write: Some(jobs::guard_write(dirs.cloned())),
            ..Operations::unlogged()
        };
    }
    Operations {
        retain_deletes: None,
        protected,
        log: Arc::new(move |_| Err(navo_l10n::tr(message).into())),
        log_without_retention: None,
        write: Some(Arc::new(|location| {
            Err(navo_core::FsError {
                location: Some(location.clone()),
                ..navo_core::FsError::new(navo_core::FsErrorKind::AccessDenied)
            })
        })),
    }
}
