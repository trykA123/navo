use std::io;
use std::sync::Arc;
use std::thread;

use super::*;

const KB: u64 = 1024;

#[cfg(target_os = "linux")]
const SAMPLE: &str = "Name:\ttest
Umask:\t0022
State:\tR (running)
VmPeak:\t  156620 kB
VmSize:\t   91096 kB
VmLck:\t       0 kB
VmPin:\t       0 kB
VmHWM:\t    8136 kB
VmRSS:\t    8136 kB
RssAnon:\t    1228 kB
RssFile:\t    6908 kB
RssShmem:\t       0 kB
VmData:\t    2576 kB
VmStk:\t     136 kB
VmExe:\t    2844 kB
VmLib:\t   12496 kB
VmPTE:\t      96 kB
VmSwap:\t       0 kB
HugetlbPages:\t       0 kB
Threads:\t2
";

fn memory(resident_bytes: u64, private_bytes: u64) -> ProcessMemory {
    ProcessMemory {
        resident_bytes,
        private_bytes,
    }
}

fn ok_probe() -> FakeProbe {
    FakeProbe(Ok(memory(300 * KB, 200 * KB)))
}

fn failing_probe() -> FakeProbe {
    FakeProbe(Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "probe denied",
    )))
}

fn report(caches: &CacheRegistry, tasks: &TaskCounters) -> MemoryReport {
    memory_report(&ok_probe(), caches, tasks)
}

fn size(bytes: u64, pinned_bytes: u64, entries: u64) -> CacheSize {
    CacheSize {
        bytes,
        pinned_bytes,
        entries,
    }
}

fn usage(name: &str, size: CacheSize, budget: u64) -> CacheUsage {
    CacheUsage {
        name: name.to_owned(),
        size,
        budget,
    }
}

fn owners(pairs: &[(&str, usize)]) -> Vec<(String, usize)> {
    pairs
        .iter()
        .map(|(owner, count)| ((*owner).to_owned(), *count))
        .collect()
}

#[cfg(target_os = "linux")]
mod proc_status {
    use super::*;

    #[test]
    fn real_sample_gives_vm_rss_and_rss_anon_in_bytes() {
        assert_eq!(
            parse_proc_status(SAMPLE),
            Some(memory(8136 * KB, 1228 * KB))
        );
    }

    #[test]
    fn missing_rss_anon_gives_none() {
        let text = SAMPLE.replace("RssAnon:\t    1228 kB\n", "");
        assert_eq!(parse_proc_status(&text), None);
    }

    #[test]
    fn missing_vm_rss_gives_none() {
        let text = SAMPLE.replace("VmRSS:\t    8136 kB\n", "");
        assert_eq!(parse_proc_status(&text), None);
    }

    #[test]
    fn malformed_number_gives_none() {
        let text = SAMPLE.replace("RssAnon:\t    1228 kB", "RssAnon:\t    12x8 kB");
        assert_eq!(parse_proc_status(&text), None);
    }

    #[test]
    fn extra_spaces_and_tabs_are_accepted() {
        let text = "VmRSS: \t  \t 8136 \t  kB\nRssAnon:\t\t   1228   kB  \n";
        assert_eq!(parse_proc_status(text), Some(memory(8136 * KB, 1228 * KB)));
    }

    #[test]
    fn proc_status_probe_returns_non_zero_values() {
        let sample = ProcStatusProbe.sample().unwrap();
        assert!(
            sample.resident_bytes > 0 && sample.private_bytes > 0,
            "{sample:?}"
        );
    }
}

mod caches {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[test]
    fn a_registered_cache_reports_its_size_and_budget() {
        let caches = CacheRegistry::default();
        caches.register("icons", 8 * KB, || size(3, 1, 2));
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", size(3, 1, 2), 8 * KB)]
        );
    }

    #[test]
    fn the_size_is_measured_when_the_report_is_made() {
        let caches = CacheRegistry::default();
        let bytes = Arc::new(AtomicU64::new(5));
        let seen = Arc::clone(&bytes);
        caches.register("icons", 8 * KB, move || {
            size(seen.load(Ordering::Relaxed), 0, 1)
        });
        bytes.store(7, Ordering::Relaxed);
        assert_eq!(caches.usage(), [usage("icons", size(7, 0, 1), 8 * KB)]);
    }

    #[test]
    fn unpinned_bytes_are_the_bytes_outside_pins() {
        assert_eq!(size(10, 4, 3).unpinned_bytes(), 6);
    }

    #[test]
    fn unpinned_bytes_never_go_below_zero() {
        assert_eq!(size(2, 4, 1).unpinned_bytes(), 0);
    }

    #[test]
    fn caches_keep_registration_order() {
        let caches = CacheRegistry::default();
        caches.register("thumbnails", 256 * KB, CacheSize::default);
        caches.register("icons", 8 * KB, CacheSize::default);
        caches.register("listings", 64 * KB, CacheSize::default);
        let names: Vec<String> = caches.usage().into_iter().map(|cache| cache.name).collect();
        assert_eq!(names, ["thumbnails", "icons", "listings"]);
    }

    #[test]
    fn the_same_name_twice_gives_two_entries() {
        let caches = CacheRegistry::default();
        caches.register("icons", 8 * KB, || size(1, 0, 1));
        caches.register("icons", 4 * KB, || size(2, 0, 1));
        assert_eq!(
            caches.usage(),
            [
                usage("icons", size(1, 0, 1), 8 * KB),
                usage("icons", size(2, 0, 1), 4 * KB)
            ]
        );
    }

    #[test]
    fn a_cloned_registry_shares_its_caches() {
        let caches = CacheRegistry::default();
        let shared = caches.clone();
        shared.register("icons", 8 * KB, CacheSize::default);
        drop(shared);
        assert_eq!(
            caches.usage(),
            [usage("icons", CacheSize::default(), 8 * KB)]
        );
    }
}

mod tasks {
    use super::*;

    #[test]
    fn counts_are_per_owner_and_sorted_by_name() {
        let tasks = TaskCounters::default();
        let _guards = [
            tasks.start("watcher"),
            tasks.start("enrich"),
            tasks.start("watcher"),
        ];
        assert_eq!(
            report(&CacheRegistry::default(), &tasks).tasks,
            owners(&[("enrich", 1), ("watcher", 2)])
        );
    }

    #[test]
    fn dropping_a_guard_decrements() {
        let tasks = TaskCounters::default();
        let first = tasks.start("watcher");
        let _second = tasks.start("watcher");
        drop(first);
        assert_eq!(
            report(&CacheRegistry::default(), &tasks).tasks,
            owners(&[("watcher", 1)])
        );
    }

    #[test]
    fn an_owner_at_zero_is_not_listed() {
        let tasks = TaskCounters::default();
        drop(tasks.start("watcher"));
        let _enrich = tasks.start("enrich");
        assert_eq!(
            report(&CacheRegistry::default(), &tasks).tasks,
            owners(&[("enrich", 1)])
        );
    }

    #[test]
    fn a_cloned_counter_shares_its_counts() {
        let tasks = TaskCounters::default();
        let shared = tasks.clone();
        let _guard = shared.start("watcher");
        drop(shared);
        assert_eq!(
            report(&CacheRegistry::default(), &tasks).tasks,
            owners(&[("watcher", 1)])
        );
    }

    fn start_and_drop_a_hundred(tasks: &TaskCounters) {
        let guards: Vec<TaskGuard> = (0..100).map(|_| tasks.start("worker")).collect();
        drop(guards);
    }

    #[test]
    fn eight_threads_of_a_hundred_guards_end_at_zero() {
        let tasks = TaskCounters::default();
        thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| start_and_drop_a_hundred(&tasks));
            }
        });
        assert_eq!(report(&CacheRegistry::default(), &tasks).tasks, []);
    }
}

mod reports {
    use super::*;

    #[test]
    fn fake_probe_returns_its_value_each_time() {
        let probe = ok_probe();
        let samples = [probe.sample().unwrap(), probe.sample().unwrap()];
        assert_eq!(samples, [memory(300 * KB, 200 * KB); 2]);
    }

    #[test]
    fn failing_fake_probe_returns_its_error_each_time() {
        let probe = failing_probe();
        let errors: Vec<(io::ErrorKind, String)> = (0..2)
            .map(|_| probe.sample().unwrap_err())
            .map(|error| (error.kind(), error.to_string()))
            .collect();
        let expected = (io::ErrorKind::PermissionDenied, "probe denied".to_owned());
        assert_eq!(errors, [expected.clone(), expected]);
    }

    #[test]
    fn report_with_a_fake_probe_has_the_process_memory() {
        let tasks = TaskCounters::default();
        let caches = CacheRegistry::default();
        caches.register("icons", 8 * KB, || size(3, 0, 1));
        let _watcher = tasks.start("watcher");
        assert_eq!(
            memory_report(&ok_probe(), &caches, &tasks),
            MemoryReport {
                process: Some(memory(300 * KB, 200 * KB)),
                probe_error: None,
                caches: vec![usage("icons", size(3, 0, 1), 8 * KB)],
                tasks: owners(&[("watcher", 1)]),
                heap: None,
                components: Vec::new(),
            }
        );
    }

    #[test]
    fn report_with_a_failing_probe_has_no_process_and_the_error_text() {
        let report = memory_report(
            &failing_probe(),
            &CacheRegistry::default(),
            &TaskCounters::default(),
        );
        assert_eq!(
            (report.process, report.probe_error),
            (None, Some("probe denied".to_owned()))
        );
    }
}

mod thread_safety {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn registries_and_guards_are_send_and_sync() {
        assert_send_sync::<CacheRegistry>();
        assert_send_sync::<TaskCounters>();
        assert_send_sync::<TaskGuard>();
    }
}
