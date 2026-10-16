use std::collections::HashMap;
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
    give_up: Instant,
}

impl Asked {
    fn answer(&self, timeout: Duration) -> Option<FillResult> {
        let deadline = self.deadline;
        let now = Instant::now();
        let until = now
            .checked_add(timeout)
            .map_or(deadline, |at| at.min(deadline));
        let slot = self
            .slot
            .result
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let wait = until.saturating_duration_since(now);
        let (slot, _) = self
            .slot
            .done
            .wait_timeout_while(slot, wait, |result| result.is_none())
            .unwrap_or_else(PoisonError::into_inner);
        match slot.as_ref() {
            Some(result) => Some(result.clone()),
            None if Instant::now() >= self.give_up => Some(Ok(Submenu::Failed)),
            None => (Instant::now() >= deadline).then_some(Ok(Submenu::Loading)),
        }
    }

    fn has_failed(&self) -> bool {
        let slot = self
            .slot
            .result
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match slot.as_ref() {
            Some(result) => matches!(result, Ok(Submenu::Failed) | Err(_)),
            None => Instant::now() >= self.give_up,
        }
    }

    fn pending(self) -> Pending<Submenu> {
        Pending::polled(move |timeout| self.answer(timeout))
    }
}

pub(super) struct Fills {
    asked: Mutex<HashMap<Vec<usize>, Asked>>,
    limit: Duration,
}

impl Default for Fills {
    fn default() -> Self {
        Self {
            asked: Mutex::default(),
            limit: BACKGROUND_LIMIT,
        }
    }
}

impl Fills {
    #[cfg(test)]
    fn with_limit(limit: Duration) -> Self {
        Self {
            asked: Mutex::default(),
            limit,
        }
    }

    pub(super) fn ask(
        &self,
        path: Vec<usize>,
        send: impl FnOnce(Vec<usize>, FillReply),
    ) -> Pending<Submenu> {
        let mut asked = self.asked.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(earlier) = asked.get(&path).filter(|earlier| !earlier.has_failed()) {
            return earlier.clone().pending();
        }
        let deadline = Instant::now() + FILL_BUDGET;
        let fresh = Asked {
            slot: Arc::default(),
            deadline,
            give_up: deadline + self.limit,
        };
        asked.insert(path.clone(), fresh.clone());
        send(path, FillReply(fresh.slot.clone()));
        fresh.pending()
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
