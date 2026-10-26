use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpgradeStep {
    InstallNew,
    AwaitNew,
    RemoveLegacyService,
    RemoveLegacyFiles,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Legacy {
    pub service: bool,
    pub files: bool,
}

impl Legacy {
    pub fn is_present(self) -> bool {
        self.service || self.files
    }
}

pub(super) fn run_upgrade(
    legacy: Legacy,
    mut execute: impl FnMut(UpgradeStep) -> io::Result<()>,
) -> io::Result<Option<io::Error>> {
    execute(UpgradeStep::InstallNew)?;
    if !legacy.is_present() {
        return Ok(None);
    }
    execute(UpgradeStep::AwaitNew).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("the new index service did not answer, so the Ferrite one was kept: {error}"),
        )
    })?;
    match execute(UpgradeStep::RemoveLegacyService) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        result => result.map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "the new index service runs, but the Ferrite one was not fully removed: {error}"
                ),
            )
        })?,
    }
    Ok(execute(UpgradeStep::RemoveLegacyFiles).err().map(|error| {
        io::Error::new(
            error.kind(),
            format!("the new index service runs; old Ferrite files were left in place: {error}"),
        )
    }))
}

pub(super) fn carry_legacy_config<C>(
    current: &io::Result<C>,
    legacy: impl FnOnce() -> Option<C>,
) -> Option<C> {
    match current {
        Err(error) if error.kind() == io::ErrorKind::NotFound => legacy(),
        _ => None,
    }
}

pub(super) fn remove_both<P: Copy>(
    products: [P; 2],
    mut remove: impl FnMut(P) -> io::Result<()>,
) -> io::Result<()> {
    let [current, legacy] = products;
    let first = remove(current);
    first.and(remove(legacy))
}

pub(super) fn await_answer(
    (limit, poll): (Duration, Duration),
    mut answers: impl FnMut() -> bool,
) -> io::Result<()> {
    let started = Instant::now();
    loop {
        if answers() {
            return Ok(());
        }
        if started.elapsed() >= limit {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the new index service pipe did not answer",
            ));
        }
        std::thread::sleep(poll);
    }
}

pub(super) struct UpgradeNotice {
    marker: Option<PathBuf>,
}

impl UpgradeNotice {
    pub fn new(marker: Option<PathBuf>) -> Self {
        Self { marker }
    }

    pub fn is_due(&self, query: impl FnOnce() -> io::Result<super::ServiceQuery>) -> bool {
        let dismissed = self
            .marker
            .as_deref()
            .is_some_and(|marker| marker.try_exists().unwrap_or(true));
        !dismissed && query().is_ok_and(|query| query.legacy_present)
    }

    pub fn dismiss(&self) -> io::Result<()> {
        match &self.marker {
            Some(marker) => rusty_store::write_atomic(marker, b""),
            None => Ok(()),
        }
    }
}
