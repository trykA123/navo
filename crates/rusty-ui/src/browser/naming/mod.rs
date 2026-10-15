use std::fmt;
use std::sync::Arc;

use rusty_core::{FsError, Location, NameError, ProviderPath, TabId, check_name, list_active_tabs};
use rusty_fs::{JobSlot, Priority, ProviderOps, volume_of};

use super::{Browser, Message};

const CASE_STEP: &str = ".ferrite-rename";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    Invalid(NameError),
    Exists,
    Unchanged,
}

impl fmt::Display for NameProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => error.fmt(f),
            Self::Exists => f.write_str("Something with this name is already here."),
            Self::Unchanged => f.write_str("The name is the same."),
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
    Rename {
        from: Location,
        to: Location,
        case_only: bool,
    },
    Create(Location),
}

impl Browser {
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), NameProblem> {
        check_name(to).map_err(NameProblem::Invalid)?;
        if from == to {
            return Err(NameProblem::Unchanged);
        }
        if self.taken(to, Some(from)) {
            return Err(NameProblem::Exists);
        }
        let (Some(source), Some(target)) = (self.here(from), self.here(to)) else {
            return Err(NameProblem::Invalid(NameError::Empty));
        };
        let change = Change::Rename {
            from: source,
            to: target,
            case_only: from.to_lowercase() == to.to_lowercase(),
        };
        self.submit_change(change, to.to_owned());
        Ok(())
    }

    pub fn new_folder(&mut self, name: &str) -> Result<(), NameProblem> {
        check_name(name).map_err(NameProblem::Invalid)?;
        if self.taken(name, None) {
            return Err(NameProblem::Exists);
        }
        let Some(target) = self.here(name) else {
            return Err(NameProblem::Invalid(NameError::Empty));
        };
        self.submit_change(Change::Create(target), name.to_owned());
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
            Ok(()) => {
                if here {
                    self.focus_after_load.insert(tab, name);
                }
                let effects = list_active_tabs(&mut self.state);
                self.run(effects);
            }
            Err(error) if here => {
                self.errors.insert(tab, error);
            }
            Err(_) => {}
        }
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
        self.queue.submit(
            slot,
            Box::new(move |_| {
                let result = apply_change(ops.as_ref(), &change);
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

fn apply_change(ops: &dyn ProviderOps, change: &Change) -> Result<(), FsError> {
    match change {
        Change::Create(target) => ops.create_folder(target),
        Change::Rename {
            from,
            to,
            case_only: false,
        } => ops.rename(from, to),
        Change::Rename {
            from,
            to,
            case_only: true,
        } => rename_case(ops, from, to),
    }
}

fn rename_case(ops: &dyn ProviderOps, from: &Location, to: &Location) -> Result<(), FsError> {
    let step = from.as_str().to_owned() + CASE_STEP;
    let Ok(step) = Location::new(step) else {
        return ops.rename(from, to);
    };
    ops.rename(from, &step)?;
    ops.rename(&step, to).inspect_err(|_| {
        let _ = ops.rename(&step, from);
    })
}

#[cfg(test)]
mod tests;
