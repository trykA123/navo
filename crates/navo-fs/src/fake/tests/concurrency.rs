use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use super::*;

const ADDED: usize = 500;

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn fake_fs_is_send_and_sync() {
    assert_send_sync::<FakeFs>();
    assert_send_sync::<CancelToken>();
}

#[test]
fn fake_fs_works_as_a_shared_provider() {
    let provider: Arc<dyn Provider> = Arc::new(FakeFs::new());
    assert_eq!(provider.id().as_str(), "fake");
}

fn add_while_listing() -> (Arc<FakeFs>, thread::Result<()>, thread::Result<()>) {
    let fs = Arc::new(with_files(0).with_batch_size(7));
    let done = Arc::new(AtomicBool::new(false));
    let reader = {
        let (fs, done) = (Arc::clone(&fs), Arc::clone(&done));
        thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                let _ = list_all(&fs, "d");
            }
        })
    };
    let writer = {
        let fs = Arc::clone(&fs);
        thread::spawn(move || {
            for i in 0..ADDED {
                fs.add_file(&format!("d/w{i}"), 1);
            }
        })
    };
    let written = writer.join();
    done.store(true, Ordering::Release);
    let read = reader.join();
    (fs, written, read)
}

#[test]
fn adding_while_listing_never_panics() {
    let (_, written, read) = add_while_listing();
    assert!(written.is_ok() && read.is_ok());
}

#[test]
fn final_listing_sees_every_added_entry() {
    let (fs, _, _) = add_while_listing();
    assert_eq!(names(&list_all(&fs, "d").1).len(), ADDED);
}
