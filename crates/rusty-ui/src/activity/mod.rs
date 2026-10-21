mod text;

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use gpui_kit::SharedString;
use rusty_core::OpKind;

use crate::job_card::{JobInfo, Pause};
use crate::job_row::JobRowState;

pub use text::percent;

pub const DONE_FOR: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActivityKind {
    Failure,
    Drag,
    Job,
    Done,
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActivityId {
    Job(u64),
    Drag,
    Background(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivityItem {
    pub id: ActivityId,
    pub kind: ActivityKind,
    pub op: Option<OpKind>,
    pub title: SharedString,
    pub chip: SharedString,
    pub detail: Option<SharedString>,
    pub progress: Option<f32>,
    pub paused: bool,
    pub can_pause: bool,
    pub can_cancel: bool,
}

impl ActivityItem {
    fn new(id: ActivityId, kind: ActivityKind, title: String) -> Self {
        let title: SharedString = title.into();
        Self {
            id,
            kind,
            op: None,
            chip: title.clone(),
            title,
            detail: None,
            progress: None,
            paused: false,
            can_pause: false,
            can_cancel: false,
        }
    }

    fn job(job: &JobInfo) -> Self {
        Self {
            op: Some(job.kind),
            chip: text::job_chip(job).into(),
            detail: Some(text::job_detail(job).into()),
            progress: Some(job.fraction()),
            paused: job.pause != Pause::Off,
            can_pause: true,
            can_cancel: true,
            ..Self::new(
                ActivityId::Job(job.id),
                ActivityKind::Job,
                text::job_title(job),
            )
        }
    }

    fn failure(job: &JobInfo, message: &SharedString) -> Self {
        Self {
            op: Some(job.kind),
            chip: text::failure_chip(job).into(),
            detail: Some(text::failure_detail(job, message).into()),
            ..Self::new(
                ActivityId::Job(job.id),
                ActivityKind::Failure,
                text::failure_title(job),
            )
        }
    }

    fn done(job: &JobInfo, undo_key: Option<&SharedString>) -> Self {
        Self {
            op: Some(job.kind),
            chip: text::done_chip(job).into(),
            detail: text::done_detail(job, undo_key).map(Into::into),
            ..Self::new(
                ActivityId::Job(job.id),
                ActivityKind::Done,
                text::done_title(job),
            )
        }
    }

    fn drag(items: usize) -> Self {
        Self {
            detail: Some(text::drag_keys().into()),
            ..Self::new(
                ActivityId::Drag,
                ActivityKind::Drag,
                text::drag_title(items),
            )
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Life {
    Lasting,
    Until(Instant),
    Held(Duration),
}

#[derive(Debug)]
struct Entry {
    item: ActivityItem,
    seq: u64,
    life: Life,
}

#[derive(Default)]
pub struct Activity {
    entries: Vec<Entry>,
    ended: HashSet<u64>,
    pinned: Option<(ActivityId, ActivityKind)>,
    seq: u64,
    undo_key: Option<SharedString>,
    batches: Vec<(ActivityId, usize)>,
}

impl Activity {
    pub fn sync(&mut self, jobs: &[JobInfo], now: Instant) {
        for job in jobs {
            self.sync_job(job, now);
        }
        let present: HashSet<u64> = jobs.iter().map(|job| job.id).collect();
        self.entries.retain(|entry| match entry.item {
            ActivityItem {
                id: ActivityId::Job(id),
                kind: ActivityKind::Job,
                ..
            } => present.contains(&id),
            _ => true,
        });
        self.ended.retain(|id| present.contains(id));
        self.tick(now);
    }

    fn sync_job(&mut self, job: &JobInfo, now: Instant) {
        if self.ended.contains(&job.id) {
            return;
        }
        let (item, life) = match &job.state {
            JobRowState::Running => (ActivityItem::job(job), Life::Lasting),
            JobRowState::Done => {
                self.note_batch(job);
                (
                    ActivityItem::done(job, self.undo_key.as_ref()),
                    Life::Until(now + DONE_FOR),
                )
            }
            JobRowState::Failed(message) => (ActivityItem::failure(job, message), Life::Lasting),
            JobRowState::Cancelled => {
                self.ended.insert(job.id);
                self.remove(ActivityId::Job(job.id));
                return;
            }
        };
        if !job.is_running() {
            self.ended.insert(job.id);
        }
        self.put(item, life);
    }

    fn note_batch(&mut self, job: &JobInfo) {
        if job.kind == OpKind::Rename && !job.rename.undo {
            self.batches.push((ActivityId::Job(job.id), job.items));
        }
    }

    pub fn set_undo_key(&mut self, key: Option<SharedString>) {
        self.undo_key = key;
    }

    pub fn shows_batch(&self, items: usize) -> bool {
        self.batches.last().is_some_and(|&(id, count)| {
            count == items
                && self
                    .entries
                    .iter()
                    .any(|entry| entry.item.id == id && entry.item.kind == ActivityKind::Done)
        })
    }

    pub fn retire_batch(&mut self, items: usize) {
        if self
            .batches
            .last()
            .is_some_and(|&(_, count)| count == items)
            && let Some((id, _)) = self.batches.pop()
        {
            self.dismiss(id);
        }
    }

    pub fn tick(&mut self, now: Instant) {
        self.entries
            .retain(|entry| !matches!(entry.life, Life::Until(at) if at <= now));
        self.unpin_ended();
    }

    pub fn next_expiry(&self) -> Option<Instant> {
        self.entries
            .iter()
            .filter_map(|entry| match entry.life {
                Life::Until(at) => Some(at),
                Life::Lasting | Life::Held(_) => None,
            })
            .min()
    }

    pub fn hold(&mut self, id: ActivityId, held: bool, now: Instant) {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.item.id == id) else {
            return;
        };
        entry.life = match (entry.life, held) {
            (Life::Until(at), true) => Life::Held(at.saturating_duration_since(now)),
            (Life::Held(left), false) => Life::Until(now + left),
            (life, _) => life,
        };
    }

    pub fn pin(&mut self, id: ActivityId) -> bool {
        let Some(entry) = self.entries.iter().find(|entry| entry.item.id == id) else {
            return false;
        };
        self.pinned = Some((id, entry.item.kind));
        true
    }

    pub fn dismiss(&mut self, id: ActivityId) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| {
            entry.item.id != id
                || !matches!(entry.item.kind, ActivityKind::Failure | ActivityKind::Done)
        });
        self.unpin_ended();
        self.entries.len() < before
    }

    pub fn set_drag(&mut self, items: Option<usize>) {
        match items {
            Some(items) => self.put(ActivityItem::drag(items), Life::Lasting),
            None => self.remove(ActivityId::Drag),
        }
    }

    pub fn set_background(&mut self, id: u64, title: SharedString, progress: Option<f32>) {
        let item = ActivityItem {
            chip: title.clone(),
            title,
            progress,
            ..ActivityItem::new(
                ActivityId::Background(id),
                ActivityKind::Background,
                String::new(),
            )
        };
        self.put(item, Life::Lasting);
    }

    pub fn end_background(&mut self, id: u64) {
        self.remove(ActivityId::Background(id));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn primary(&self) -> Option<&ActivityItem> {
        self.ordered().next()
    }

    pub fn rest(&self) -> impl Iterator<Item = &ActivityItem> {
        self.ordered().skip(1)
    }

    fn ordered(&self) -> impl Iterator<Item = &ActivityItem> {
        let mut entries: Vec<&Entry> = self.entries.iter().collect();
        entries.sort_by_key(|entry| {
            let is_pinned = self.pinned == Some((entry.item.id, entry.item.kind));
            (!is_pinned, entry.item.kind, std::cmp::Reverse(entry.seq))
        });
        entries.into_iter().map(|entry| &entry.item)
    }

    fn put(&mut self, item: ActivityItem, life: Life) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.item.id == item.id)
        {
            if entry.item.kind != item.kind {
                entry.life = life;
                self.seq += 1;
                entry.seq = self.seq;
            }
            entry.item = item;
        } else {
            self.seq += 1;
            self.entries.push(Entry {
                item,
                seq: self.seq,
                life,
            });
        }
        self.unpin_ended();
    }

    fn remove(&mut self, id: ActivityId) {
        self.entries.retain(|entry| entry.item.id != id);
        self.unpin_ended();
    }

    fn unpin_ended(&mut self) {
        let is_live = |(id, kind)| {
            self.entries
                .iter()
                .any(|entry| entry.item.id == id && entry.item.kind == kind)
        };
        if self.pinned.is_some_and(|pinned| !is_live(pinned)) {
            self.pinned = None;
        }
    }
}
