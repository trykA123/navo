use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rusty_core::shell_menu::Submenu;

use super::{FILL_BUDGET, MenuError, Pending};

pub(super) const BACKGROUND_LIMIT: Duration = Duration::from_secs(10);
pub(super) const POLL: Duration = Duration::from_millis(10);

type FillResult = Result<Submenu, MenuError>;

#[derive(Default)]
struct Slot {
    result: Mutex<Option<FillResult>>,
    done: Condvar,
}

impl Slot {
    fn finish(&self, result: FillResult) {
        let mut slot = self.result.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(result);
            self.done.notify_all();
        }
    }

    fn answer(&self, deadline: Instant, timeout: Duration) -> Option<FillResult> {
        let now = Instant::now();
        let until = now
            .checked_add(timeout)
            .map_or(deadline, |at| at.min(deadline));
        let slot = self.result.lock().unwrap_or_else(PoisonError::into_inner);
        let wait = until.saturating_duration_since(now);
        let (slot, _) = self
            .done
            .wait_timeout_while(slot, wait, |result| result.is_none())
            .unwrap_or_else(PoisonError::into_inner);
        match slot.as_ref() {
            Some(result) => Some(result.clone()),
            None => (Instant::now() >= deadline).then_some(Ok(Submenu::Loading)),
        }
    }
}

pub(super) struct FillReply(Arc<Slot>);

impl FillReply {
    pub(super) fn send(&self, result: FillResult) {
        self.0.finish(result);
    }
}

impl Drop for FillReply {
    fn drop(&mut self) {
        self.0.finish(Err(MenuError::Stopped));
    }
}

#[derive(Clone)]
struct Asked {
    slot: Arc<Slot>,
    deadline: Instant,
}

impl Asked {
    fn pending(self) -> Pending<Submenu> {
        Pending::polled(move |timeout| self.slot.answer(self.deadline, timeout))
    }
}

#[derive(Default)]
pub(super) struct Fills(Mutex<HashMap<Vec<usize>, Asked>>);

impl Fills {
    pub(super) fn ask(
        &self,
        path: Vec<usize>,
        send: impl FnOnce(Vec<usize>, FillReply),
    ) -> Pending<Submenu> {
        let mut asked = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = match asked.entry(path) {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let path = entry.key().clone();
                let fresh = Asked {
                    slot: Arc::default(),
                    deadline: Instant::now() + FILL_BUDGET,
                };
                entry.insert(fresh.clone());
                send(path, FillReply(fresh.slot.clone()));
                fresh
            }
        };
        entry.pending()
    }
}

pub(super) struct Background {
    session: u64,
    path: Vec<usize>,
    reply: FillReply,
    deadline: Instant,
}

impl Background {
    pub(super) fn new(session: u64, path: Vec<usize>, reply: FillReply, now: Instant) -> Self {
        Self {
            session,
            path,
            reply,
            deadline: now + BACKGROUND_LIMIT,
        }
    }
}

pub(super) fn settle(
    fills: &mut Vec<Background>,
    now: Instant,
    mut probe: impl FnMut(u64, &[usize]) -> Option<FillResult>,
) -> Option<Duration> {
    fills.retain(|fill| {
        let result = probe(fill.session, &fill.path)
            .or_else(|| (now >= fill.deadline).then_some(Ok(Submenu::Failed)));
        result.map(|result| fill.reply.send(result)).is_none()
    });
    fills
        .iter()
        .map(|fill| fill.deadline.saturating_duration_since(now).min(POLL))
        .min()
}

#[cfg(test)]
mod tests;
