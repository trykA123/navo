use std::sync::{Mutex, Weak};

use rusty_diag::components::{MemoryUse, Reading};

use super::Hosts;

pub(super) const NAME: &str = "walk index";

pub(super) fn read(hosts: &Weak<Mutex<Hosts>>) -> Reading {
    let Some(hosts) = hosts.upgrade() else {
        return Reading::Gone;
    };
    let Ok(hosts) = hosts.try_lock() else {
        return Reading::Busy;
    };
    let mut total = MemoryUse::default();
    for root in &hosts.roots {
        let table = root.host.table();
        let Ok(table) = table.try_read() else {
            return Reading::Busy;
        };
        total.bytes += table.memory_bytes() as u64;
        total.items += table.len() as u64;
    }
    Reading::Use(total)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use rusty_fs::FakeFs;
    use rusty_store::StoreDirs;

    use super::*;
    use crate::search_hooks::index::create_hosts;

    fn walked(fs: FakeFs, folder: &std::path::Path) -> Arc<Mutex<Hosts>> {
        let root = fs.location("tree");
        let hosts = create_hosts(Some(StoreDirs::in_profile(folder)), Arc::new(fs)).unwrap();
        hosts.lock().unwrap().start_root(root);
        let weak = Arc::downgrade(&hosts);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(read(&weak), Reading::Use(read) if read.items >= 3) {
            assert!(Instant::now() < deadline, "the walk did not finish");
            std::thread::yield_now();
        }
        hosts
    }

    fn tree() -> FakeFs {
        let fs = FakeFs::new();
        fs.add_folder("tree");
        fs.add_file("tree/a.txt", 1);
        fs.add_file("tree/b.txt", 1);
        fs
    }

    #[test]
    fn a_walked_root_reports_its_name_table() {
        let folder = tempfile::tempdir().unwrap();
        let hosts = walked(tree(), folder.path());
        let Reading::Use(read) = read(&Arc::downgrade(&hosts)) else {
            panic!("the index was busy");
        };
        assert!(read.bytes > 0);
    }

    #[test]
    fn a_held_lock_reads_as_busy_and_a_dropped_index_as_gone() {
        let folder = tempfile::tempdir().unwrap();
        let hosts = walked(tree(), folder.path());
        let weak = Arc::downgrade(&hosts);
        let guard = hosts.lock().unwrap();
        assert_eq!(read(&weak), Reading::Busy);
        let table = guard.roots[0].host.table();
        drop(guard);
        let write = table.write().unwrap();
        assert_eq!(read(&weak), Reading::Busy);
        drop(write);
        drop(hosts);
        assert_eq!(read(&weak), Reading::Gone);
    }
}
