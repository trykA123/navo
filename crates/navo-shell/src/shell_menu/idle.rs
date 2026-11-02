use std::collections::VecDeque;

use super::fill::FillReply;

pub(super) struct Prefill {
    pub session: u64,
    pub path: Vec<usize>,
    pub reply: FillReply,
}

/// Work the shell thread does only while no request waits: prefills of open menus.
#[derive(Default)]
pub(super) struct Idle {
    prefills: VecDeque<Prefill>,
}

impl Idle {
    pub(super) fn prefill(&mut self, prefill: Prefill) {
        self.prefills.push_back(prefill);
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

    pub(super) fn next(&mut self) -> Option<Prefill> {
        self.prefills.pop_front()
    }
}

#[cfg(test)]
mod tests;
