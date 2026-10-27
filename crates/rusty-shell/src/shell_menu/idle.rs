use std::collections::VecDeque;

use rusty_core::shell_menu::{ItemKind, ShellMenu, Submenu};

use super::fill::FillReply;
use super::warm::Warm;

pub(super) struct Prefill {
    pub session: u64,
    pub path: Vec<usize>,
    pub reply: FillReply,
}

pub(super) enum Job {
    Prefill(Prefill),
    Warm(Warm),
}

/// Work the shell thread does only while no request waits: prefills of open menus first, then warming.
#[derive(Default)]
pub(super) struct Idle {
    prefills: VecDeque<Prefill>,
    warm: VecDeque<Warm>,
}

impl Idle {
    pub(super) fn warming(steps: Vec<Warm>) -> Self {
        Self {
            prefills: VecDeque::new(),
            warm: steps.into(),
        }
    }

    pub(super) fn prefill(&mut self, prefill: Prefill) {
        self.prefills.push_back(prefill);
    }

    pub(super) fn warm_later(&mut self, step: Warm) {
        self.warm.push_back(step);
    }

    /// Takes a waiting prefill out of the queue so the caller can run it now.
    pub(super) fn promote(&mut self, session: u64, path: &[usize]) -> Option<Prefill> {
        let at = self
            .prefills
            .iter()
            .position(|prefill| prefill.session == session && prefill.path == path)?;
        self.prefills.remove(at)
    }

    pub(super) fn close(&mut self, session: u64) {
        self.prefills.retain(|prefill| prefill.session != session);
    }

    pub(super) fn next(&mut self) -> Option<Job> {
        self.prefills
            .pop_front()
            .map(Job::Prefill)
            .or_else(|| self.warm.pop_front().map(Job::Warm))
    }
}

/// The top-level submenus a handler fills only when they open.
pub(super) fn lazy_tops(menu: &ShellMenu) -> Vec<Vec<usize>> {
    menu.items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item.kind, ItemKind::Submenu(Submenu::Lazy)))
        .map(|(index, _)| vec![index])
        .collect()
}

#[cfg(test)]
mod tests;
