use std::collections::TryReserveError;

use super::NameTable;

const MIN_GROWTH: usize = 1_024;
const MIN_NAME_GROWTH: usize = 16 * 1_024;

/// Grows a full column by an eighth, so a long build copies about a tenth of its entries.
pub(super) fn grow_column<T>(column: &mut Vec<T>) {
    if column.len() == column.capacity() {
        column.reserve_exact((column.len() / 8).max(MIN_GROWTH));
    }
}

/// Grows the name arena by an eighth when the next name does not fit in its spare capacity.
pub(super) fn grow_names(names: &mut String, needed: usize) {
    if names.capacity() - names.len() < needed {
        names.reserve_exact((names.len() / 8).max(MIN_NAME_GROWTH).max(needed));
    }
}

/// Spare capacity an unsized build keeps, as a divisor of the length.
const UNSIZED_SLACK: usize = 16;
/// A sized build keeps the hint's sixteenth plus a sixteenth of shrinkage, so a volume slightly
/// smaller than the old index does not copy a column while both indexes are held.
const SIZED_SLACK: usize = 8;

pub(super) fn trim_column<T>(column: &mut Vec<T>, slack: usize) {
    if column.capacity() - column.len() > column.len() / slack {
        column.shrink_to_fit();
    }
}

fn trim_names(names: &mut String, slack: usize) {
    if names.capacity() - names.len() > names.len() / slack {
        names.shrink_to_fit();
    }
}

impl NameTable {
    /// Reserves total capacities for a build: slots and every entry column, the folder counts,
    /// and the name arena.
    pub fn try_reserve_total(
        &mut self,
        entries: usize,
        folders: usize,
        name_bytes: usize,
    ) -> Result<(), TryReserveError> {
        self.names
            .try_reserve_exact(name_bytes.saturating_sub(self.names.len()))?;
        let additional = entries.saturating_sub(self.offsets.len());
        self.offsets.try_reserve_exact(additional)?;
        self.parents.try_reserve_exact(additional)?;
        self.flags.try_reserve_exact(additional)?;
        self.bytes.try_reserve_exact(additional)?;
        self.counts
            .try_reserve_exact(folders.saturating_sub(self.counts.len()))?;
        self.children.try_reserve_total(entries, folders)?;
        Ok(())
    }

    /// Number of live folders; deleted folders keep a zeroed row in the counts column.
    pub fn folder_count(&self) -> usize {
        self.counts
            .iter()
            .filter(|&&(id, _)| self.is_live(id))
            .count()
    }

    /// Drops spare capacity only where it exceeds a sixteenth of the column.
    pub fn trim_slack(&mut self) {
        self.trim_slack_over(UNSIZED_SLACK);
    }

    /// Drops spare capacity only where it exceeds an eighth of the column; for sized builds.
    pub fn trim_sized_slack(&mut self) {
        self.trim_slack_over(SIZED_SLACK);
    }

    fn trim_slack_over(&mut self, slack: usize) {
        trim_names(&mut self.names, slack);
        trim_column(&mut self.offsets, slack);
        trim_column(&mut self.parents, slack);
        trim_column(&mut self.flags, slack);
        trim_column(&mut self.bytes, slack);
        trim_column(&mut self.counts, slack);
        self.children.trim_slack(slack);
    }

    /// Bytes of live names, each with its terminating NUL. Dead names in the arena do not count.
    pub fn live_name_bytes(&self) -> usize {
        (0..self.slots())
            .filter(|&id| self.is_live(id))
            .map(|id| self.name_unchecked(id).len() + 1)
            .sum()
    }
}

#[cfg(test)]
impl NameTable {
    pub(crate) fn spares_within_slack(&self) -> bool {
        let within = |capacity: usize, len: usize| capacity - len <= len / 16;
        within(self.names.capacity(), self.names.len())
            && within(self.offsets.capacity(), self.offsets.len())
            && within(self.parents.capacity(), self.parents.len())
            && within(self.flags.capacity(), self.flags.len())
            && within(self.bytes.capacity(), self.bytes.len())
            && within(self.counts.capacity(), self.counts.len())
            && self
                .children
                .spares()
                .into_iter()
                .all(|(capacity, len)| within(capacity, len))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::print_stdout,
        reason = "growth tests fail on broken fixtures and print the measured peak"
    )]

    use super::super::{Flags, NO_PARENT, NameTable};
    use crate::heap_probe::Probe;

    const ENTRIES: u32 = 1_000_000;

    fn table(entries: u32) -> NameTable {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        for number in 1..entries {
            table
                .push(root, &format!("entry-{number:07}.txt"), Flags::NONE)
                .unwrap();
        }
        table.shrink_to_fit();
        table
    }

    fn step(len: usize) -> usize {
        (len / 8).max(1_024)
    }

    #[test]
    fn one_push_after_an_exact_fit_grows_by_an_eighth() {
        let mut table = table(ENTRIES);
        let probe = Probe::start();
        table.push(0, "pushed.txt", Flags::FOLDER).unwrap();
        let measured = probe.read();
        let per_entry = measured.peak / ENTRIES as usize;
        println!(
            "growth push: peak {} KiB, {per_entry} B/entry",
            measured.peak >> 10
        );
        assert!(per_entry <= 28, "{per_entry} B/entry");
    }

    #[test]
    fn every_column_grows_by_at_most_its_step() {
        let mut table = table(100_000);
        let old = OldLengths::read(&table);
        let needed = "pushed.txt".len() + 1;
        table.push(0, "pushed.txt", Flags::FOLDER).unwrap();
        assert!(
            table.names.capacity() - old.names <= (old.names / 8).max(16 * 1_024).max(needed),
            "names {} -> {}",
            old.names,
            table.names.capacity()
        );
        for (name, (before, after)) in [
            ("offsets", (old.offsets, table.offsets.capacity())),
            ("parents", (old.parents, table.parents.capacity())),
            ("flags", (old.flags, table.flags.capacity())),
            ("bytes", (old.bytes, table.bytes.capacity())),
            ("counts", (old.counts, table.counts.capacity())),
        ] {
            assert!(after - before <= step(before), "{name} {before} -> {after}");
        }
        let children = table.children.capacities();
        for (name, (before, after)) in [
            ("children.first", (old.children[0], children[0])),
            ("children.next", (old.children[1], children[1])),
            ("children.previous", (old.children[2], children[2])),
        ] {
            assert!(after - before <= step(before), "{name} {before} -> {after}");
        }
    }

    #[test]
    fn try_reserve_total_reaches_the_requested_totals() {
        let mut table = NameTable::new();
        table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        table.try_reserve_total(100_000, 10_000, 100_000).unwrap();
        assert_eq!(table.offsets.capacity(), 100_000);
        assert_eq!(table.parents.capacity(), 100_000);
        assert_eq!(table.flags.capacity(), 100_000);
        assert_eq!(table.bytes.capacity(), 100_000);
        assert_eq!(table.counts.capacity(), 10_000);
        assert_eq!(table.names.capacity(), 100_000);
        let [(first, _), (next, _), (previous, _)] = table.children.spares();
        assert!((10_000..=100_000).contains(&first));
        assert_eq!(next, 100_000);
        assert_eq!(previous, 100_000);
    }

    #[test]
    fn trim_slack_keeps_a_small_spare_and_drops_a_large_one() {
        let mut small = NameTable::new();
        small.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        small.try_reserve_total(100_000, 100_000, 100_000).unwrap();
        small.trim_slack();
        let len = small.slots() as usize;
        assert!(small.names.capacity() - small.names.len() <= len / 16);
        for capacity in [
            small.offsets.capacity(),
            small.parents.capacity(),
            small.flags.capacity(),
            small.bytes.capacity(),
            small.counts.capacity(),
        ] {
            assert!(capacity <= len + len / 16, "{capacity} for {len}");
        }
        for capacity in small.children.capacities() {
            assert!(capacity <= len + len / 16, "{capacity} for {len}");
        }

        let mut table = table(1_000);
        let names = table.live_name_bytes();
        table.try_reserve_total(1_010, 1_000, names + 10).unwrap();
        let kept = table.offsets.capacity();
        table.trim_slack();
        assert_eq!(table.offsets.capacity(), kept);
        assert_eq!(table.names.capacity(), names + 10);
    }

    /// The 1,000-entry table with total capacities reserved as a sized build would.
    fn reserved(extra_entries: usize, extra_names: usize) -> NameTable {
        let mut table = table(1_000);
        let (names, slots) = (table.names.len(), table.offsets.len());
        table
            .try_reserve_total(slots + extra_entries, 1, names + extra_names)
            .unwrap();
        table
    }

    #[test]
    fn a_sized_trim_keeps_a_reserve_the_build_fell_just_short_of() {
        // A hint a sixteenth over the old index, for a build a few bytes smaller than it.
        let built = table(1_000);
        let (entries, names) = (built.offsets.len() / 16 + 1, built.names.len() / 16 + 8);
        let mut sized = reserved(entries, names);
        let kept = (sized.names.capacity(), sized.offsets.capacity());
        sized.trim_sized_slack();
        assert_eq!((sized.names.capacity(), sized.offsets.capacity()), kept);

        let mut trimmed = reserved(entries, names);
        trimmed.trim_slack();
        assert!(trimmed.names.capacity() < kept.0);
        assert!(trimmed.offsets.capacity() < kept.1);
    }

    #[test]
    fn a_sized_trim_keeps_an_eighth_and_drops_more() {
        let names = table(1_000).names.len();
        let mut at = reserved(0, names / 8);
        at.trim_sized_slack();
        assert_eq!(at.names.capacity(), names + names / 8);
        let mut over = reserved(0, names / 8 + 1);
        over.trim_sized_slack();
        assert!(over.names.capacity() < names + names / 8);
    }

    #[test]
    fn live_name_bytes_ignores_old_names_after_a_rename() {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let file = table.push(root, "old.txt", Flags::NONE).unwrap();
        table.rename(file, root, "new-name.txt").unwrap();
        assert_eq!(
            table.live_name_bytes(),
            "C:".len() + 1 + "new-name.txt".len() + 1
        );
    }

    struct OldLengths {
        names: usize,
        offsets: usize,
        parents: usize,
        flags: usize,
        bytes: usize,
        counts: usize,
        children: [usize; 3],
    }

    impl OldLengths {
        fn read(table: &NameTable) -> Self {
            Self {
                names: table.names.len(),
                offsets: table.offsets.capacity(),
                parents: table.parents.capacity(),
                flags: table.flags.capacity(),
                bytes: table.bytes.capacity(),
                counts: table.counts.capacity(),
                children: table.children.capacities(),
            }
        }
    }
}
