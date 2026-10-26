use std::fmt;
use std::sync::Arc;

use rusty_core::{FsError, Location, NameError, ProviderPath, TabId, check_name};
use rusty_fs::{JobSlot, Priority, ProviderOps, volume_of};

use super::{Browser, Message};

mod rename;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    Invalid(NameError),
    Exists,
    Unchanged,
    Busy,
}

impl fmt::Display for NameProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => error.fmt(f),
            Self::Exists => f.write_str("Something with this name is already here."),
            Self::Unchanged => f.write_str("The name is the same."),
            Self::Busy => f.write_str(&rusty_l10n::tr("rename-busy")),
        }
    }
}

pub(super) struct Named {
    tab: TabId,
    folder: Location,
    name: String,
    result: Result<(), FsError>,
}

enum Change {
    CreateFolder(Location),
    CreateFile(Location),
}

impl Browser {
    pub fn new_folder(&mut self, name: &str) -> Result<(), NameProblem> {
        self.create(name, Change::CreateFolder)
    }

    pub fn new_file(&mut self, name: &str) -> Result<(), NameProblem> {
        self.create(name, Change::CreateFile)
    }

    fn create(&mut self, name: &str, change: fn(Location) -> Change) -> Result<(), NameProblem> {
        check_name(name).map_err(NameProblem::Invalid)?;
        if self.taken(name, None) {
            return Err(NameProblem::Exists);
        }
        let Some(target) = self.here(name) else {
            return Err(NameProblem::Invalid(NameError::Empty));
        };
        self.submit_change(change(target), name.to_owned());
        Ok(())
    }

    pub(super) fn accept_named(&mut self, named: Named) -> bool {
        let Named {
            tab,
            folder,
            name,
            result,
        } = named;
        let here = self.state.tab(tab).is_some_and(|t| t.location() == &folder);
        match result {
            Ok(()) if here => {
                self.reveal_name(tab, &name);
                self.focus_after_load.insert(tab, name);
            }
            Err(error) if here => {
                self.late_errors.insert(tab, error);
            }
            _ => {}
        }
        self.relist_answering();
        true
    }

    fn here(&self, name: &str) -> Option<Location> {
        self.state
            .active_tab()
            .location()
            .join(&ProviderPath::from_text(name))
            .ok()
    }

    fn taken(&self, name: &str, except: Option<&str>) -> bool {
        let tab = self.state.active_tab();
        let listing = self
            .full
            .get(&tab.id())
            .map(|full| &full.listing)
            .or_else(|| tab.listing());
        let wanted = name.to_lowercase();
        listing.is_some_and(|listing| {
            listing
                .entries()
                .iter()
                .any(|e| e.name.to_lowercase() == wanted && Some(e.name.as_str()) != except)
        })
    }

    fn submit_change(&mut self, change: Change, name: String) {
        let tab = self.state.active_tab();
        let (tab, folder) = (tab.id(), tab.location().clone());
        let (ops, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let slot = JobSlot {
            volume: volume_of(&folder),
            priority: Priority::Visible,
            owner: self.owner.id(),
        };
        self.in_flight += 1;
        let write = self.operations.write.clone();
        self.queue.submit(
            slot,
            Box::new(move |_| {
                let result = apply_change(ops.as_ref(), &change, write.as_ref());
                let named = Named {
                    tab,
                    folder,
                    name,
                    result,
                };
                let _ = sender.send(Message::Named(named));
                wake();
            }),
        );
    }
}

fn apply_change(
    ops: &dyn ProviderOps,
    change: &Change,
    write: Option<&super::WriteFactory>,
) -> Result<(), FsError> {
    let (Change::CreateFolder(target) | Change::CreateFile(target)) = change;
    let _lease = write.map(|write| write(target)).transpose()?;
    match change {
        Change::CreateFolder(target) => ops.create_folder(target),
        Change::CreateFile(target) => ops.create_file(target),
    }
}

#[cfg(test)]
mod tests;
