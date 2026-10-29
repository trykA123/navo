use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use rusty_cache::DiskListings;

use crate::jobs::maintenance::MaintenanceTimer;
use crate::search_hooks::Visits;

const EXIT_CLOSE: Duration = Duration::from_secs(1);

pub(super) fn quitter(
    (disk, visits): (Option<DiskListings>, Option<Arc<Visits>>),
    cleanup: crate::profile::Cleanup,
    running: Option<crate::crash::Running>,
    maintenance: MaintenanceTimer,
) -> Rc<dyn Fn()> {
    Rc::new(move || {
        if let Some(visits) = &visits {
            visits.stop();
        }
        maintenance.stop();
        if let Some(disk) = &disk {
            let _ = disk.close(EXIT_CLOSE);
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
