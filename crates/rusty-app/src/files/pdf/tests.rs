use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::schedule;

#[test]
fn the_warm_up_runs_once_and_off_the_calling_thread() {
    let started = AtomicBool::new(false);
    let (sender, receiver) = mpsc::channel();
    let mut handles = Vec::new();
    for _ in 0..3 {
        let sender = sender.clone();
        handles.extend(schedule(&started, Duration::ZERO, move || {
            sender.send(thread::current().id()).unwrap();
        }));
    }
    drop(sender);
    assert_eq!(handles.len(), 1);
    for handle in handles {
        handle.join().unwrap();
    }
    let ran: Vec<_> = receiver.iter().collect();
    assert_eq!(ran.len(), 1);
    assert_ne!(ran[0], thread::current().id());
}

#[test]
fn scheduling_returns_before_the_delay_ends_and_the_job_waits_for_it() {
    let started = AtomicBool::new(false);
    let (sender, receiver) = mpsc::channel();
    let delay = Duration::from_millis(300);
    let begun = Instant::now();
    let handle = schedule(&started, delay, move || {
        sender.send(Instant::now()).unwrap();
    })
    .unwrap();
    assert!(begun.elapsed() < delay);
    let ran = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(ran.duration_since(begun) >= delay);
    handle.join().unwrap();
}
