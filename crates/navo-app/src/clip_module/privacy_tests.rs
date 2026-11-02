//! The privacy canary: copied text, a path or a secret must reach neither the logs, the event
//! ring, a crash record, a report nor (after a delete) the database files. Each test runs in
//! its own process (nextest) because the spy is the global tracing subscriber.

use std::fmt::Debug;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use navo_shell::CaptureContent;
use navo_ui::clip_palette::{ClipId, ClipMode, ClipboardHooks};

use super::tests::{Rig, wait_until_len};
use check::{Hay, assert_clean, debug_hay, files_in, hay, leaks};
use spy::Spy;

mod bytes;
mod check;
mod controls;
mod flows;
mod secret;
mod secret_hooks;
mod spy;
mod store_errors;

const CANARY: &str = "CANARY-7f3a9c-clip";

/// What a test saw through `Debug` and `Display`.
#[derive(Default)]
struct Seen(Vec<Hay>);

impl Seen {
    fn note(&mut self, name: &str, value: &dyn Debug) {
        self.0.push(debug_hay(name, value));
    }

    fn text(&mut self, name: &str, text: &str) {
        self.0.push(hay(name, text));
    }

    /// The service's snapshot, its rows, its last failure and the palette notice.
    fn service(&mut self, rig: &Rig) {
        if let Some((service, snapshot)) = rig.module.peek().live {
            self.note("snapshot", &snapshot);
            self.note("history rows", &snapshot.history);
            self.note("saved rows", &snapshot.saved);
            self.note("last failure", &service.last_error());
        }
        let notice = rig.module.search("", ClipMode::All, None).notice;
        self.text("notice", &notice.unwrap_or_default());
    }
}

#[derive(Clone, Copy)]
enum Payload {
    Text,
    Paths,
}

impl Payload {
    fn capture(self, rig: &Rig) {
        let content = match self {
            Self::Text => CaptureContent::Text(format!("{CANARY} first line\nsecond line")),
            Self::Paths => CaptureContent::Files {
                paths: vec![
                    PathBuf::from(format!("/work/{CANARY}/plan.txt")),
                    PathBuf::from("/work/other.txt"),
                ],
                cut: false,
            },
        };
        assert!(rig.emit_content(content));
        wait_until_len(rig, 1);
    }
}

fn only_row(rig: &Rig) -> ClipId {
    let found = rig.module.search(CANARY, ClipMode::All, None);
    assert_eq!(found.rows.len(), 1);
    found.rows[0].id
}

fn stored_bytes(rig: &Rig) -> Vec<Hay> {
    files_in(&rig.data)
}

/// Waits for the worker to drop `needle` from every file of the store.
fn wait_gone(rig: &Rig, needle: &str) {
    let mut found = Vec::new();
    for _ in 0..1000 {
        found = leaks(needle, &stored_bytes(rig));
        if found.is_empty() {
            return;
        }
        sleep(Duration::from_millis(5));
    }
    panic!("{needle} is still stored in {found:?}");
}

/// Crashes once, builds the report, then checks every place a leak could land.
fn finish(spy: &Spy, seen: &Seen, needle: &str) {
    finish_all(spy, seen, &[needle]);
}

/// Like `finish` for several needles; the crash and the report happen once.
fn finish_all(spy: &Spy, seen: &Seen, needles: &[&str]) {
    spy.crash("privacy test crash");
    let (raw, ring, files) = (spy.raw(), spy.ring(), spy.files());
    let (crashes, report) = (spy.crash_records(), spy.report());
    for needle in needles {
        assert_clean(needle, &raw);
        assert_clean(needle, &ring);
        assert_clean(needle, &files);
        assert_clean(needle, &crashes);
        assert_clean(needle, &report);
        assert_clean(needle, &seen.0);
    }
}
