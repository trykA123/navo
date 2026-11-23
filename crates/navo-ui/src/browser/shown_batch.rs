//! D7 shown-folder sizes in one totals request: one size job asks the source for every shown
//! folder at once; only the folders it cannot answer are measured one by one.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use navo_core::{Location, Tab, TabId};
use navo_fs::{FolderSize, JobKind, JobSlot, OwnerGuard, OwnerId, Priority, volume_of};

use super::measure::{Asked, Asker, FolderTotalsQuery, Measured, TotalsState};
use super::{Browser, Message};

/// What the batch job found: `ready` rows to show now, `rest` names to measure one by one.
pub(super) struct Answered {
    tab: TabId,
    ready: Vec<Measured>,
    rest: Vec<String>,
}

impl Browser {
    /// Asks the totals source once, on one idle size job, for the shown folders `names` of
    /// `tab` that nobody asked for yet; the answer comes back as `Message::ShownBatch`.
    pub(super) fn measure_shown(&mut self, tab: &Tab, names: Vec<String>) {
        let (tab_id, folder) = (tab.id(), tab.location().clone());
        let Some(work) = self.sizes.work.get_mut(&tab_id) else {
            return;
        };
        let Some(owner) = work.shown_owner.as_ref().map(OwnerGuard::id) else {
            return;
        };
        let asked: Vec<(String, Location)> = names
            .into_iter()
            .filter(|name| !work.asked.contains_key(name))
            .filter_map(|name| tab.entry_location(&name).ok().map(|target| (name, target)))
            .collect();
        if asked.is_empty() {
            return;
        }
        let keys: Vec<String> = asked.iter().map(|(name, _)| name.clone()).collect();
        let slot = JobSlot {
            volume: volume_of(&folder),
            priority: Priority::Idle,
            owner,
        };
        let totals = Arc::clone(&self.folder_totals);
        let (sender, wake) = (self.sender.clone(), Arc::clone(&self.wake));
        work.shown_pending += 1;
        let job = self.pools.queue(JobKind::Size).submit_then(
            slot,
            move |_| answer(&totals, tab_id, &folder, asked),
            move |answered| {
                let _ = sender.send(Message::ShownBatch(owner, answered));
                wake();
            },
        );
        for key in keys {
            let asker = Asker::Shown;
            work.asked.insert(key, Asked { job, asker });
        }
    }

    /// Shows the batch's ready sizes in one drain and measures the rest one by one; an answer
    /// for a listing the tab has left, or for cancelled shown sizes, is dropped.
    pub(super) fn accept_shown_batch(&mut self, owner: OwnerId, answered: Answered) -> bool {
        let Answered { tab, ready, rest } = answered;
        let Some(work) = self
            .sizes
            .work
            .get_mut(&tab)
            .filter(|work| work.owns(owner))
        else {
            return false;
        };
        // The batch counted once; each ready row is settled on its own by `accept_sizes`.
        work.shown_pending = work.shown_pending.saturating_sub(1) + ready.len();
        let mut alone = Vec::new();
        for name in rest {
            let asker = match work.asked.get(&name).map(|asked| asked.asker) {
                Some(Asker::Shown) => Asker::Shown,
                Some(Asker::ShownThenSpace) => Asker::Space,
                _ => continue,
            };
            work.asked.remove(&name);
            alone.push((name, asker));
        }
        let changed = self.accept_sizes(ready.into_iter().map(|row| (owner, row)).collect());
        let Some(tab) = self.state.tab(tab).cloned() else {
            return changed;
        };
        for (name, asker) in alone {
            self.measure_entry(&tab, name, asker);
        }
        changed
    }
}

/// Runs on the size job: one `query_many` for all `asked` folders. A complete total is ready;
/// anything else, a short answer or a panicking source leaves the folder to be measured alone.
fn answer(
    totals: &FolderTotalsQuery,
    tab: TabId,
    folder: &Location,
    asked: Vec<(String, Location)>,
) -> Answered {
    let targets: Vec<Location> = asked.iter().map(|(_, target)| target.clone()).collect();
    let states = catch_unwind(AssertUnwindSafe(|| totals.query_many(&targets))).unwrap_or_default();
    let mut states = states.into_iter();
    let (mut ready, mut rest) = (Vec::new(), Vec::new());
    for (name, _) in asked {
        match states.next() {
            Some(TotalsState::Ready(found)) if found.complete => ready.push(Measured {
                tab,
                folder: folder.clone(),
                name,
                size: Some(FolderSize::apparent_only(found.bytes)),
            }),
            _ => rest.push(name),
        }
    }
    Answered { tab, ready, rest }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use navo_core::Location;
    use navo_fs::{FakeFs, FakeOp, VolumeKind};

    use crate::browser::tests::{browser_at, settle};
    use crate::browser::{Browser, FolderTotals, FolderTotalsSource, TotalsState};

    /// Answers by folder name and counts single and batch calls apart.
    #[derive(Default)]
    struct Batched {
        singles: AtomicUsize,
        batches: AtomicUsize,
    }

    /// `gone` folders have no totals, `part` folders an incomplete total, the rest 42 bytes.
    fn answer(folder: &Location) -> TotalsState {
        let path = folder.as_str();
        let complete = !path.ends_with("part");
        if path.ends_with("gone") {
            return TotalsState::Unavailable;
        }
        TotalsState::Ready(FolderTotals {
            bytes: 42,
            files: 1,
            folders: 0,
            complete,
        })
    }

    impl FolderTotalsSource for Batched {
        fn query(&self, folder: &Location) -> TotalsState {
            self.singles.fetch_add(1, Ordering::SeqCst);
            answer(folder)
        }

        fn query_many(&self, folders: &[Location]) -> Vec<TotalsState> {
            self.batches.fetch_add(1, Ordering::SeqCst);
            folders.iter().map(answer).collect()
        }
    }

    /// A browser in `list` with the shown sizes on, the batch source set and every job done.
    fn shown(names: &[String]) -> (Browser, Arc<FakeFs>, Arc<Batched>) {
        let fs = FakeFs::new();
        for name in names {
            fs.add_file(&format!("list/{name}/one.bin"), 7);
        }
        let fs = Arc::new(fs);
        let mut browser = browser_at(&fs, "list");
        settle(&mut browser);
        browser.set_volume_kinds(Arc::new(|_| VolumeKind::Ssd));
        let source = Arc::new(Batched::default());
        browser.set_folder_totals(source.clone());
        browser.set_measures_shown_folders(true);
        settle(&mut browser);
        (browser, fs, source)
    }

    fn sizes(browser: &Browser) -> Vec<(String, u64)> {
        let tab = browser.state().active_tab().id();
        browser
            .details(tab)
            .iter()
            .map(|row| (row.name.to_text(), row.size))
            .collect()
    }

    #[test]
    fn thirty_ready_folders_take_one_source_call_and_no_walk() {
        let names: Vec<String> = (0..30).map(|n| format!("f{n:02}")).collect();
        let (browser, fs, source) = shown(&names);
        assert_eq!(source.batches.load(Ordering::SeqCst), 1, "one batch call");
        assert_eq!(
            source.singles.load(Ordering::SeqCst),
            0,
            "no folder asked alone"
        );
        assert!(sizes(&browser).iter().all(|(_, size)| *size == 42));
        for name in &names {
            assert_eq!(
                fs.calls(&format!("list/{name}"), FakeOp::List),
                0,
                "{name} walked"
            );
        }
    }

    #[test]
    fn unanswered_and_incomplete_folders_are_measured_alone() {
        let names: Vec<String> = ["a", "b-gone", "c-part", "d"].map(String::from).to_vec();
        let (browser, fs, source) = shown(&names);
        assert_eq!(source.batches.load(Ordering::SeqCst), 1);
        assert_eq!(
            source.singles.load(Ordering::SeqCst),
            2,
            "only the two left over"
        );
        let expected = [("a", 42), ("b-gone", 7), ("c-part", 7), ("d", 42)]
            .map(|(name, size)| (name.to_owned(), size));
        assert_eq!(sizes(&browser), expected);
        assert_eq!(
            fs.calls("list/a", FakeOp::List),
            0,
            "a Ready folder is not walked"
        );
    }

    /// A batch path that panics; single asks have no totals.
    struct Broken;

    impl FolderTotalsSource for Broken {
        fn query(&self, _: &Location) -> TotalsState {
            TotalsState::Unavailable
        }

        fn query_many(&self, _: &[Location]) -> Vec<TotalsState> {
            panic!("batch path broke")
        }
    }

    #[test]
    fn a_panicking_batch_leaves_every_folder_to_be_measured_alone() {
        let names: Vec<String> = ["a", "b"].map(String::from).to_vec();
        let (mut browser, fs, _) = shown(&names);
        browser.set_folder_totals(Arc::new(Broken));
        browser.set_measures_shown_folders(false);
        browser.set_measures_shown_folders(true);
        settle(&mut browser);
        assert_eq!(sizes(&browser), [("a".to_owned(), 7), ("b".to_owned(), 7)]);
        assert_eq!(
            fs.calls("list/a", FakeOp::List),
            1,
            "walked once after the panic"
        );
    }
}
