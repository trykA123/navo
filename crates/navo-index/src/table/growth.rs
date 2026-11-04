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

pub(super) fn trim_column<T>(column: &mut Vec<T>) {
    if column.capacity() - column.len() > column.len() / 16 {
        column.shrink_to_fit();
    }
}

fn trim_names(names: &mut String) {
    if names.capacity() - names.len() > names.len() / 16 {
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
        self.children.try_reserve_total(entries)?;
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
        trim_names(&mut self.names);
        trim_column(&mut self.offsets);
        trim_column(&mut self.parents);
        trim_column(&mut self.flags);
        trim_column(&mut self.bytes);
        trim_column(&mut self.counts);
        self.children.trim_slack();
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
                .capacities()
                .into_iter()
                .all(|capacity| within(capacity, self.offsets.len()))
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
        for capacity in table.children.capacities() {
            assert_eq!(capacity, 100_000);
        }
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
