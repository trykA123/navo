use std::sync::Arc;

use rusty_core::Location;
use rusty_fs::ProviderOps;
use rusty_store::StoreDirs;
use rusty_ui::browser::Operations;

use crate::jobs::maintenance::MaintenanceTimer;
use crate::{files, jobs, metrics, rename_hooks, retained_hooks};

pub(super) struct ProfileServices {
    pub(super) memory: Option<files::SharedDb>,
    pub(super) files: rusty_ui::window_view::FileHooks,
    pub(super) operations: Operations,
    pub(super) rename: rusty_ui::browser::RenameHooks,
    pub(super) retained: Arc<dyn rusty_ui::browser::RetainedHooks>,
    pub(super) maintenance: MaintenanceTimer,
}

pub(super) fn profile_services(
    (dirs, journal): (Option<&StoreDirs>, Option<jobs::SharedJournal>),
    home: &Location,
    experimental: bool,
    local: &Arc<dyn ProviderOps>,
) -> ProfileServices {
    let (operations, journal) = operations((dirs, journal), home, experimental, local);
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
) -> (Operations, Option<jobs::SharedJournal>) {
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
    let logged = Operations {
        retain_deletes: experimental.then(|| {
            Arc::new(move || jobs::retains_deletes(&retention_dirs))
                as Arc<dyn Fn() -> Option<rusty_core::retention::RetentionPolicy> + Send + Sync>
        }),
        write: Some(jobs::write_factory(dirs.clone(), experimental)),
        log: if experimental {
            jobs::retaining_factory(Arc::clone(&journal), dirs.clone(), local.clone())
        } else {
            jobs::profile_factory(Arc::clone(&journal), dirs.clone())
        },
        protected,
    };
    (logged, Some(journal))
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
        log: Arc::new(move |_| Err(rusty_l10n::tr(message))),
        write: Some(Arc::new(|location| {
            Err(rusty_core::FsError {
                location: Some(location.clone()),
                ..rusty_core::FsError::new(rusty_core::FsErrorKind::AccessDenied)
            })
        })),
    }
}
