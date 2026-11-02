use super::NameTable;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TableMemory {
    pub names: usize,
    pub tree: usize,
    pub totals: usize,
}

impl TableMemory {
    pub fn total(self) -> usize {
        self.names + self.tree + self.totals
    }
}

impl NameTable {
    pub fn memory_parts(&self) -> TableMemory {
        TableMemory {
            names: self.names.capacity() + self.offsets.capacity() * size_of::<u32>(),
            tree: self.parents.capacity() * size_of::<u32>()
                + self.flags.capacity() * size_of::<u16>()
                + self.children.memory_bytes(),
            totals: self.bytes.capacity() * size_of::<u64>()
                + self.counts.capacity() * size_of::<(u32, [u32; 2])>(),
        }
    }

    pub fn memory_bytes(&self) -> usize {
        self.memory_parts().total()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{Flags, NO_PARENT};

    fn table(files: usize) -> NameTable {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let folder = table.push(root, "work", Flags::FOLDER).unwrap();
        for at in 0..files {
            let id = table
                .push(folder, &format!("file-{at}.txt"), Flags::NONE)
                .unwrap();
            table.set_size(id, at as u64).unwrap();
        }
        table
    }

    #[test]
    fn an_empty_table_holds_no_memory() {
        assert_eq!(NameTable::new().memory_parts(), TableMemory::default());
    }

    #[test]
    fn a_table_with_entries_reports_every_part() {
        let parts = table(10).memory_parts();
        assert!(parts.names >= 10 * "file-0.txt".len());
        assert!(parts.tree >= 12 * (size_of::<u32>() * 4 + size_of::<u16>()));
        assert!(parts.totals >= 12 * size_of::<u64>());
        assert_eq!(parts.total(), table(10).memory_bytes());
    }

    #[test]
    fn parts_grow_with_the_table() {
        let mut last = table(0).memory_parts();
        for files in [10, 100, 1_000, 10_000] {
            let next = table(files).memory_parts();
            assert!(next.names > last.names, "names at {files}");
            assert!(next.tree > last.tree, "tree at {files}");
            assert!(next.totals > last.totals, "totals at {files}");
            last = next;
        }
    }
}
