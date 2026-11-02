use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use navo_cache::DiskListings;

use crate::jobs::maintenance::MaintenanceTimer;
use crate::search_hooks::Visits;

const EXIT_CLOSE: Duration = Duration::from_secs(1);

pub(super) fn quitter(
    (disk, visits, sessions): (
        Option<DiskListings>,
        Option<Arc<Visits>>,
        Option<crate::jobs::SessionHolds>,
    ),
    cleanup: crate::profile::Cleanup,
    running: Option<crate::crash::Running>,
    maintenance: MaintenanceTimer,
) -> Rc<dyn Fn()> {
    Rc::new(move || {
        if let Some(visits) = &visits {
            visits.stop();
        }
        maintenance.stop();
        crate::files::stop_pdf();
        if let Some(disk) = &disk {
            let _ = disk.close(EXIT_CLOSE);
        }
        if let Some(sessions) = &sessions {
            sessions.release(&navo_fs::LocalFs::new());
        }
        if let Some(journal) = sessions.as_ref().map(crate::jobs::SessionHolds::journal)
            && let Err(error) = crate::jobs::lock(&journal).database().checkpoint()
        {
            tracing::warn!(%error, "journal checkpoint failed at quit");
        }
        if let Some(running) = &running
            && let Err(error) = running.finish()
        {
            tracing::warn!(%error, "cannot remove the running marker");
        }
        cleanup.on_quit();
        crate::logging::flush();
    })
}
