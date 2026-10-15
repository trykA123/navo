use std::io;
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

fn usage(name: &str, bytes: u64, budget: u64) -> CacheUsage {
    CacheUsage {
        name: name.to_owned(),
        bytes,
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
    use super::*;

    #[test]
    fn set_replaces_the_size() {
        let caches = CacheRegistry::default();
        let icons = caches.register("icons", 8 * KB);
        icons.set(5);
        icons.set(3);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", 3, 8 * KB)]
        );
    }

    #[test]
    fn add_accumulates() {
        let caches = CacheRegistry::default();
        let icons = caches.register("icons", 8 * KB);
        icons.add(5);
        icons.add(7);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", 12, 8 * KB)]
        );
    }

    #[test]
    fn sub_subtracts() {
        let caches = CacheRegistry::default();
        let icons = caches.register("icons", 8 * KB);
        icons.set(10);
        icons.sub(4);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", 6, 8 * KB)]
        );
    }

    #[test]
    fn sub_saturates_at_zero() {
        let caches = CacheRegistry::default();
        let icons = caches.register("icons", 8 * KB);
        icons.set(3);
        icons.sub(10);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", 0, 8 * KB)]
        );
    }

    #[test]
    fn two_caches_are_reported_apart() {
        let caches = CacheRegistry::default();
        let icons = caches.register("icons", 8 * KB);
        let listings = caches.register("listings", 64 * KB);
        icons.set(1);
        listings.set(2);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", 1, 8 * KB), usage("listings", 2, 64 * KB)]
        );
    }

    #[test]
    fn a_dropped_cache_disappears() {
        let caches = CacheRegistry::default();
        let icons = caches.register("icons", 8 * KB);
        let listings = caches.register("listings", 64 * KB);
        drop(icons);
        listings.set(2);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("listings", 2, 64 * KB)]
        );
    }

    #[test]
    fn a_cache_stays_while_a_cloned_handle_lives() {
        let caches = CacheRegistry::default();
        let icons = caches.register("icons", 8 * KB);
        let clone = icons.clone();
        drop(icons);
        clone.set(4);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", 4, 8 * KB)]
        );
    }

    #[test]
    fn caches_keep_registration_order() {
        let caches = CacheRegistry::default();
        let _thumbnails = caches.register("thumbnails", 256 * KB);
        let _icons = caches.register("icons", 8 * KB);
        let _listings = caches.register("listings", 64 * KB);
        let names: Vec<String> = report(&caches, &TaskCounters::default())
            .caches
            .into_iter()
            .map(|cache| cache.name)
            .collect();
        assert_eq!(names, ["thumbnails", "icons", "listings"]);
    }

    #[test]
    fn the_same_name_twice_gives_two_entries() {
        let caches = CacheRegistry::default();
        let first = caches.register("icons", 8 * KB);
        let second = caches.register("icons", 4 * KB);
        first.set(1);
        second.set(2);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", 1, 8 * KB), usage("icons", 2, 4 * KB)]
        );
    }

    #[test]
    fn a_cloned_registry_shares_its_caches() {
        let caches = CacheRegistry::default();
        let _icons = caches.clone().register("icons", 8 * KB);
        assert_eq!(
            report(&caches, &TaskCounters::default()).caches,
            [usage("icons", 0, 8 * KB)]
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
        let _guard = tasks.clone().start("watcher");
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
        let icons = caches.register("icons", 8 * KB);
        icons.set(3);
        let _watcher = tasks.start("watcher");
        assert_eq!(
            memory_report(&ok_probe(), &caches, &tasks),
            MemoryReport {
                process: Some(memory(300 * KB, 200 * KB)),
                probe_error: None,
                caches: vec![usage("icons", 3, 8 * KB)],
                tasks: owners(&[("watcher", 1)]),
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
    fn registries_handles_and_guards_are_send_and_sync() {
        assert_send_sync::<CacheRegistry>();
        assert_send_sync::<CacheHandle>();
        assert_send_sync::<TaskCounters>();
        assert_send_sync::<TaskGuard>();
    }
}
