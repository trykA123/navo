use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::files;
use rusty_core::brand;

pub(super) struct Locks {
    pub sole: bool,
    session: File,
    startup: Mutex<Option<File>>,
}

impl Locks {
    pub fn acquire(data: &Path) -> io::Result<Arc<Self>> {
        let startup = Self::startup(data)?;
        let session = rusty_store::StoreDirs::lock_in(data);
        files::regular(&session)?;
        let instance = crate::jobs::claim(&session)
            .ok_or_else(|| io::Error::other("cannot claim the database session"))?;
        Ok(Arc::new(Self {
            sole: instance.sole,
            session: instance.into_file(),
            startup: Mutex::new(Some(startup)),
        }))
    }

    pub fn startup(data: &Path) -> io::Result<File> {
        let path = data.join(format!("{}.startup-lock", brand::SLUG));
        files::regular(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.lock()?;
        Ok(file)
    }

    pub fn share(&self) -> io::Result<()> {
        let mut startup = self
            .startup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if startup.is_none() {
            return Ok(());
        }
        if self.sole {
            self.session.unlock()?;
            self.session.lock_shared()?;
        }
        startup.take();
        Ok(())
    }
}
