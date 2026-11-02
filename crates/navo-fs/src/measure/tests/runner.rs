use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use super::profile::Every;
use crate::{FakeFs, FolderProfile, IoQueue, ProfileState, Provider, spawn_profile};

fn fs() -> Arc<FakeFs> {
    let fs = FakeFs::new().with_batch_size(1);
    for index in 0..20 {
        fs.add_file(&format!("d/f{index}/x.bin"), 1);
    }
    Arc::new(fs)
}

#[test]
fn a_spawned_profile_reports_done_on_the_queue() {
    let (queue, fs) = (IoQueue::new(), fs());
    let (send, receive) = mpsc::channel();
    let location = fs.location("d");
    let provider: Arc<dyn Provider> = fs;
    let _handle = spawn_profile(&queue, provider, location, move |p: &FolderProfile| {
        if p.state.is_final() {
            let _ = send.send(p.clone());
        }
    });
    let last = receive.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((last.state, last.totals.files), (ProfileState::Done, 20));
}

#[test]
fn dropping_the_handle_stops_the_profile() {
    let (queue, fs) = (IoQueue::new(), fs());
    let (started, wait_started) = mpsc::channel();
    let (go, wait_go) = mpsc::channel::<()>();
    let (finished, wait_finished) = mpsc::channel();
    let location = fs.location("d");
    let provider: Arc<dyn Provider> = fs;
    let mut first = true;
    let sink = Every(Duration::ZERO, move |p: &FolderProfile| {
        if first {
            first = false;
            let _ = started.send(());
            let _ = wait_go.recv_timeout(Duration::from_secs(10));
        }
        if p.state.is_final() {
            let _ = finished.send(p.state);
        }
        ControlFlow::Continue(())
    });
    let handle = spawn_profile(&queue, provider, location, sink);
    wait_started.recv_timeout(Duration::from_secs(10)).unwrap();
    drop(handle);
    go.send(()).unwrap();
    let state = wait_finished.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(state, ProfileState::Stopped);
}
