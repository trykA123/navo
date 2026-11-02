use super::*;

fn start_all(scheduler: &mut Scheduler, volume: &str) -> Vec<JobId> {
    std::iter::from_fn(|| scheduler.start_next_on(&vol(volume))).collect()
}

fn push_many(scheduler: &mut Scheduler, volume: &str, count: usize) -> Vec<JobId> {
    (0..count)
        .map(|_| scheduler.push(vol(volume), Priority::Visible, A))
        .collect()
}

#[test]
fn custom_limits_apply_to_each_volume_kind() {
    let limits = Limits {
        ssd: 3,
        hdd: 2,
        network: 5,
        removable: 4,
    };
    for (kind, expected) in [
        (VolumeKind::Ssd, 3),
        (VolumeKind::Hdd, 2),
        (VolumeKind::Network, 5),
        (VolumeKind::Removable, 4),
        (VolumeKind::Unknown, 2),
    ] {
        let mut scheduler = Scheduler::with(limits);
        scheduler.set_volume(vol("V:"), kind);
        push_many(&mut scheduler, "V:", 10);
        assert_eq!(start_all(&mut scheduler, "V:").len(), expected);
    }
}

#[test]
fn higher_priority_starts_first() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    let idle = s.push(vol("C:"), Priority::Idle, A);
    let prefetch = s.push(vol("C:"), Priority::Prefetch, A);
    let visible = s.push(vol("C:"), Priority::Visible, A);
    assert_eq!(start_all(&mut s, "C:"), vec![visible, prefetch, idle]);
}

#[test]
fn equal_priorities_start_in_push_order() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    let jobs = push_many(&mut s, "C:", 3);
    assert_eq!(start_all(&mut s, "C:"), jobs);
}

#[test]
fn start_next_on_returns_none_when_nothing_is_queued() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    assert_eq!(s.start_next_on(&vol("C:")), None);
}

#[test]
fn hdd_runs_one_job_at_a_time() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    push_many(&mut s, "D:", 3);
    assert_eq!(start_all(&mut s, "D:").len(), 1);
}

#[test]
fn ssd_runs_eight_jobs_at_a_time() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    push_many(&mut s, "C:", 10);
    assert_eq!(start_all(&mut s, "C:").len(), 8);
}

#[test]
fn unknown_volume_runs_one_job_at_a_time() {
    let mut s = Scheduler::new();
    push_many(&mut s, "E:", 3);
    assert_eq!(start_all(&mut s, "E:").len(), 1);
}

#[test]
fn running_and_queued_count_per_volume() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    push_many(&mut s, "D:", 3);
    s.start_next_on(&vol("D:"));
    assert_eq!((s.running(&vol("D:")), s.queued(&vol("D:"))), (1, 2));
}

#[test]
fn start_next_picks_the_higher_priority_across_volumes() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    s.set_volume(vol("D:"), VolumeKind::Ssd);
    s.push(vol("C:"), Priority::Idle, A);
    let visible = s.push(vol("D:"), Priority::Visible, A);
    assert_eq!(s.start_next(), Some((vol("D:"), visible)));
}

#[test]
fn start_next_picks_the_oldest_among_equal_priorities_across_volumes() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    s.set_volume(vol("D:"), VolumeKind::Ssd);
    let older = s.push(vol("D:"), Priority::Prefetch, A);
    s.push(vol("C:"), Priority::Prefetch, A);
    assert_eq!(s.start_next(), Some((vol("D:"), older)));
}

#[test]
fn busy_volume_does_not_block_another_volume() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    s.set_volume(vol("C:"), VolumeKind::Ssd);
    push_many(&mut s, "D:", 2);
    s.start_next_on(&vol("D:"));
    let other = s.push(vol("C:"), Priority::Idle, A);
    assert_eq!(s.start_next(), Some((vol("C:"), other)));
}

#[test]
fn start_next_returns_none_when_every_volume_is_full() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    push_many(&mut s, "D:", 2);
    s.start_next();
    assert_eq!(s.start_next(), None);
}

#[test]
fn finish_frees_a_slot() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    let jobs = push_many(&mut s, "D:", 2);
    s.start_next_on(&vol("D:"));
    s.finish(jobs[0]);
    assert_eq!(s.start_next_on(&vol("D:")), Some(jobs[1]));
}

#[test]
fn finish_of_a_running_job_returns_true() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    let job = s.push(vol("D:"), Priority::Visible, A);
    s.start_next_on(&vol("D:"));
    assert!(s.finish(job));
}

#[test]
fn second_finish_returns_false() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    let job = s.push(vol("D:"), Priority::Visible, A);
    s.start_next_on(&vol("D:"));
    s.finish(job);
    assert!(!s.finish(job));
}

#[test]
fn finish_of_a_queued_job_returns_false() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    let job = s.push(vol("D:"), Priority::Visible, A);
    assert!(!s.finish(job));
}

#[test]
fn finish_of_an_unknown_job_returns_false() {
    let mut s = Scheduler::new();
    assert!(!s.finish(JobId(999)));
}

fn lowered_with_three_running() -> (Scheduler, Vec<JobId>) {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    let jobs = push_many(&mut s, "C:", 4);
    for _ in 0..3 {
        s.start_next_on(&vol("C:"));
    }
    s.set_volume(vol("C:"), VolumeKind::Hdd);
    (s, jobs)
}

#[test]
fn lowering_the_limit_keeps_running_jobs() {
    let (s, _) = lowered_with_three_running();
    assert_eq!(s.running(&vol("C:")), 3);
}

#[test]
fn lowering_the_limit_blocks_starts_while_above_it() {
    let (mut s, jobs) = lowered_with_three_running();
    s.finish(jobs[0]);
    s.finish(jobs[1]);
    assert_eq!(s.start_next_on(&vol("C:")), None);
}

#[test]
fn lowering_the_limit_allows_starts_once_below_it() {
    let (mut s, jobs) = lowered_with_three_running();
    for job in &jobs[..3] {
        s.finish(*job);
    }
    assert_eq!(s.start_next_on(&vol("C:")), Some(jobs[3]));
}

#[test]
fn raising_the_limit_allows_the_next_start() {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    let jobs = push_many(&mut s, "D:", 2);
    s.start_next_on(&vol("D:"));
    s.set_volume(vol("D:"), VolumeKind::Ssd);
    assert_eq!(s.start_next_on(&vol("D:")), Some(jobs[1]));
}

#[test]
fn reprioritise_moves_a_queued_job_ahead() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    let idle = s.push(vol("C:"), Priority::Idle, A);
    let prefetch = s.push(vol("C:"), Priority::Prefetch, A);
    s.reprioritise(idle, Priority::Visible);
    assert_eq!(start_all(&mut s, "C:"), vec![idle, prefetch]);
}

#[test]
fn reprioritise_keeps_the_job_age() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    let first = s.push(vol("C:"), Priority::Visible, A);
    let middle = s.push(vol("C:"), Priority::Idle, A);
    let last = s.push(vol("C:"), Priority::Visible, A);
    s.reprioritise(middle, Priority::Visible);
    assert_eq!(start_all(&mut s, "C:"), vec![first, middle, last]);
}

#[test]
fn reprioritise_of_a_queued_job_returns_true() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    let job = s.push(vol("C:"), Priority::Idle, A);
    assert!(s.reprioritise(job, Priority::Visible));
}

#[test]
fn reprioritise_of_a_running_job_returns_false() {
    let mut s = scheduler_with("C:", VolumeKind::Ssd);
    let job = s.push(vol("C:"), Priority::Idle, A);
    s.start_next_on(&vol("C:"));
    assert!(!s.reprioritise(job, Priority::Visible));
}

fn two_owners() -> (Scheduler, [JobId; 4]) {
    let mut s = scheduler_with("D:", VolumeKind::Hdd);
    let running = s.push(vol("D:"), Priority::Visible, A);
    s.start_next_on(&vol("D:"));
    let queued_a1 = s.push(vol("D:"), Priority::Idle, A);
    let queued_b = s.push(vol("D:"), Priority::Visible, B);
    let queued_a2 = s.push(vol("D:"), Priority::Visible, A);
    (s, [running, queued_a1, queued_b, queued_a2])
}

#[test]
fn cancel_owner_lists_queued_jobs_in_push_order() {
    let (mut s, [_, a1, _, a2]) = two_owners();
    assert_eq!(s.cancel_owner(A).queued, vec![a1, a2]);
}

#[test]
fn cancel_owner_lists_running_jobs() {
    let (mut s, [running, ..]) = two_owners();
    assert_eq!(s.cancel_owner(A).running, vec![running]);
}

#[test]
fn cancel_owner_leaves_running_jobs_running() {
    let (mut s, _) = two_owners();
    s.cancel_owner(A);
    assert_eq!(s.running(&vol("D:")), 1);
}

#[test]
fn running_job_of_a_cancelled_owner_still_finishes() {
    let (mut s, [running, ..]) = two_owners();
    s.cancel_owner(A);
    assert!(s.finish(running));
}

#[test]
fn cancel_owner_leaves_other_owners_queued() {
    let (mut s, _) = two_owners();
    s.cancel_owner(A);
    assert_eq!(s.queued(&vol("D:")), 1);
}

#[test]
fn cancelled_queued_jobs_never_start() {
    let (mut s, [running, _, b, _]) = two_owners();
    s.cancel_owner(A);
    s.finish(running);
    s.set_volume(vol("D:"), VolumeKind::Ssd);
    assert_eq!(start_all(&mut s, "D:"), vec![b]);
}

#[test]
fn cancel_owner_without_jobs_returns_nothing() {
    let (mut s, _) = two_owners();
    assert_eq!(s.cancel_owner(OwnerId(9)), Cancelled::default());
}
