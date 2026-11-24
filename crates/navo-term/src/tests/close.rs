//! Unix close-path checks: SIGHUP escalation, a stalled sink and a full input queue.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::{Recorded, Recorder};
use crate::{PtyBackend, Session, ShellKind, ShellSpec, Sink, Size, TermError};

const SIZE: Size = Size { cols: 80, rows: 24 };
/// Well below the recorder's 20 s: each of these ends in well under a second when correct.
const LIMIT: Duration = Duration::from_secs(5);

fn sh(script: &str) -> ShellSpec {
    ShellSpec {
        kind: ShellKind::Other,
        program: PathBuf::from("/bin/sh"),
        args: vec![String::from("-c"), String::from(script)],
        label: String::from("sh"),
    }
}

#[test]
fn a_shell_that_ignores_sighup_still_ends_and_is_reaped_on_drop() {
    let sink = Arc::new(Recorder::default());
    let script = sh("trap '' HUP; printf ready; while :; do read line; done");
    let session = Session::spawn(&script, Path::new("/"), SIZE, sink.clone()).unwrap();
    sink.wait_for(|seen| seen.printed("ready"));
    let start = Instant::now();
    drop(session);
    assert!(start.elapsed() < Duration::from_millis(100), "drop blocked");
    let seen = sink.wait_within(LIMIT, |seen| seen.exit.is_some());
    assert_eq!(seen.exits, 1);
}

/// A sink whose `output` blocks until the test opens the gate.
#[derive(Default)]
struct Stalled {
    open: Mutex<bool>,
    opened: Condvar,
    inner: Recorder,
}

impl Stalled {
    fn release(&self) {
        *self.open.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.opened.notify_all();
    }
}

impl Sink for Stalled {
    fn output(&self, bytes: &[u8]) {
        let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        drop(
            self.opened
                .wait_while(open, |open| !*open)
                .unwrap_or_else(PoisonError::into_inner),
        );
        self.inner.output(bytes);
    }

    fn exited(&self, code: Option<u32>) {
        self.inner.exited(code);
    }
}

#[test]
fn a_stalled_sink_does_not_stop_the_reader_draining_the_shell() {
    let sink = Arc::new(Stalled::default());
    let script = sh("head -c 1000000 /dev/zero | tr '\\0' x; exit 7");
    let backend = PtyBackend::spawn(&script, Path::new("/"), SIZE, sink.clone()).unwrap();
    assert!(
        backend.reader_ends_within(LIMIT),
        "the reader waited for a stalled sink"
    );
    sink.release();
    let seen = sink.inner.wait_within(LIMIT, |seen| seen.exit.is_some());
    assert_eq!(seen.exit, Some(Some(7)));
    let xs = seen.output.iter().filter(|&&byte| byte == b'x').count();
    assert_eq!(
        xs, 1_000_000,
        "output under the buffer limit is never dropped"
    );
}

#[test]
fn a_full_input_queue_refuses_writes_without_blocking_and_drop_still_ends_the_shell() {
    let sink = Arc::new(Recorder::default());
    let script = sh("stty raw -echo; printf ready; exec sleep 30");
    let mut session = Session::spawn(&script, Path::new("/"), SIZE, sink.clone()).unwrap();
    sink.wait_for(|seen| seen.printed("ready"));
    let chunk = vec![b'a'; 4096];
    let start = Instant::now();
    let full = (0..10_000).find_map(|_| match session.write(&chunk) {
        Ok(()) => None,
        Err(TermError::InputFull) => Some(()),
        Err(other) => panic!("unexpected write error: {other}"),
    });
    assert!(full.is_some(), "the input queue never filled");
    assert!(start.elapsed() < LIMIT, "writes blocked the caller");
    drop(session);
    let seen: Recorded = sink.wait_within(LIMIT, |seen| seen.exit.is_some());
    assert_eq!(seen.exits, 1);
}
