use rusty_core::ProviderPath;

use super::WatchChange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawKind {
    Created,
    Removed,
    Modified,
    MovedFrom(u32),
    MovedTo(u32),
}

#[derive(Default)]
pub(crate) struct Translate {
    moved_from: Option<(u32, ProviderPath)>,
}

impl Translate {
    pub(crate) fn feed(&mut self, kind: RawKind, name: ProviderPath, out: &mut Vec<WatchChange>) {
        if let RawKind::MovedTo(cookie) = kind {
            let change = self.moved_to(cookie, name, out);
            out.push(change);
            return;
        }
        self.flush(out);
        match kind {
            RawKind::Created => out.push(WatchChange::Created(name)),
            RawKind::Removed => out.push(WatchChange::Removed(name)),
            RawKind::Modified => out.push(WatchChange::Modified(name)),
            RawKind::MovedFrom(cookie) => self.moved_from = Some((cookie, name)),
            RawKind::MovedTo(_) => {}
        }
    }

    pub(crate) fn flush(&mut self, out: &mut Vec<WatchChange>) {
        if let Some((_, name)) = self.moved_from.take() {
            out.push(WatchChange::Removed(name));
        }
    }

    fn moved_to(
        &mut self,
        cookie: u32,
        name: ProviderPath,
        out: &mut Vec<WatchChange>,
    ) -> WatchChange {
        match self.moved_from.take() {
            Some((from_cookie, from)) if from_cookie == cookie => {
                WatchChange::Renamed { from, to: name }
            }
            Some((_, from)) => {
                out.push(WatchChange::Removed(from));
                WatchChange::Created(name)
            }
            None => WatchChange::Created(name),
        }
    }
}
