use std::thread::scope;

const CHUNK: u64 = 4096;

use super::parallel::{Hooked, no_chunk_hook};
use super::pause::{Paused, clean_copy, start_with};
use super::*;

const WORKERS: usize = 4;

fn once_past(bytes: u64) -> impl FnMut(&Progress) -> bool + Send {
    let mut fired = false;
    move |progress| {
        let fire = !fired && progress.bytes_done >= bytes;
        fired |= fire;
        fire
    }
}

#[test]
fn a_parallel_pause_is_reported_and_resume_ends_like_a_clean_run() {
    let fs = three_files();
    let plan = copy_three(&fs);
    scope(|s| {
        let paused = Paused {
            ops: &fs,
            plan: &plan,
            workers: WORKERS,
        };
        let job = start_with(s, paused, once_past(1));
        let _ = job.next_pause().expect("the job pauses");
        job.token.resume();
        assert_eq!(job.finish(), JobOutcome::Done);
    });
    assert_eq!(
        (tree_of(&fs, ""), leftovers(&fs)),
        (clean_copy(), Vec::new())
    );
}

fn pause_inside_the_first_copy(path: &str, cancel: &CancelToken) {
    if path == "src/one.txt" {
        cancel.pause();
    }
}

#[test]
fn a_parallel_pause_holds_a_copy_within_one_chunk() {
    let fs = three_files();
    let plan = copy_three(&fs);
    let ops = Hooked {
        fs: &fs,
        before_copy: &pause_inside_the_first_copy,
        on_chunk: &no_chunk_hook,
    };
    scope(|s| {
        let paused = Paused {
            ops: &ops,
            plan: &plan,
            workers: WORKERS,
        };
        let job = start_with(s, paused, |_: &Progress| false);
        let _ = job.next_pause().expect("the job pauses");
        assert!(size(&fs, &temp("dst/one.txt")) <= Some(CHUNK));
        assert!(!exists(&fs, "dst/one.txt"));
        job.token.resume();
        assert_eq!(job.finish(), JobOutcome::Done);
    });
    assert_eq!(
        (tree_of(&fs, ""), leftovers(&fs)),
        (clean_copy(), Vec::new())
    );
}

#[test]
fn cancel_while_a_parallel_job_is_paused_ends_promptly_with_no_temporary_file() {
    let fs = three_files();
    let plan = copy_three(&fs);
    let ops = Hooked {
        fs: &fs,
        before_copy: &pause_inside_the_first_copy,
        on_chunk: &no_chunk_hook,
    };
    let done = scope(|s| {
        let paused = Paused {
            ops: &ops,
            plan: &plan,
            workers: WORKERS,
        };
        let job = start_with(s, paused, |_: &Progress| false);
        let _ = job.next_pause().expect("the job pauses");
        job.token.cancel();
        assert!(job.ends_promptly());
        let done = job.done();
        assert_eq!(job.finish(), JobOutcome::Cancelled);
        done
    });
    let placed: Vec<String> = tree_of(&fs, "dst").into_iter().map(|e| e.0).collect();
    let mut journalled: Vec<String> = ["one.txt", "two.txt", "three.txt"]
        .into_iter()
        .zip(0_u32..)
        .filter(|(_, seq)| done.contains(seq))
        .map(|(name, _)| name.to_owned())
        .collect();
    journalled.sort();
    assert_eq!((placed, leftovers(&fs)), (journalled, Vec::new()));
}

#[test]
fn a_parallel_job_paused_and_resumed_many_times_copies_everything() {
    let fs = three_files();
    let plan = copy_three(&fs);
    let pauses = scope(|s| {
        let paused = Paused {
            ops: &fs,
            plan: &plan,
            workers: WORKERS,
        };
        let job = start_with(s, paused, |_: &Progress| true);
        let mut pauses = 0;
        while job.next_pause().is_some() {
            pauses += 1;
            job.token.resume();
        }
        assert_eq!(job.finish(), JobOutcome::Done);
        pauses
    });
    assert!(pauses >= 3, "{pauses}");
    assert_eq!(
        (tree_of(&fs, ""), leftovers(&fs)),
        (clean_copy(), Vec::new())
    );
}
