use std::path::PathBuf;
use std::sync::OnceLock;

use navo_fs::ProviderOps;
use navo_store::{JournalError, StoreDirs};
use navo_ui::browser::LogFactory;

use super::{SharedJournal, profile_factory, recover_profile, trim};

fn dirs() -> &'static StoreDirs {
    static DIRS: OnceLock<StoreDirs> = OnceLock::new();
    DIRS.get_or_init(|| {
        let root: PathBuf = std::env::temp_dir().join("navo-default-profile-tests");
        let dirs = StoreDirs::in_profile(&root);
        dirs.ensure()
            .expect("the test profile folder can be created");
        dirs
    })
}

pub fn log_factory(journal: SharedJournal) -> LogFactory {
    profile_factory(journal, dirs().clone())
}

pub fn recover(journal: &SharedJournal, ops: &dyn ProviderOps) -> Result<u32, JournalError> {
    let report = recover_profile(journal, ops, (&dirs().data, &[]), false)?;
    trim::at_startup(journal, &dirs().data, ops)?;
    Ok(report.unfinished)
}
