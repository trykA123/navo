//! Tests of the production write path (P1 A1): the diff hooks a screen gets copy through the
//! journal's host into the real file, and `announce` toasts a start-up refusal.

use std::fs;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use sidewise_core::{Location, ProviderPath};
use sidewise_ui::compare_diff::{
    CompareOptions, CopyChange, CopyDirection, CopyResult, CopySink, DiffHooks, DiffSink,
    DiffUpdate, PairRequest,
};

use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::TempDir;
use crate::screens::compare_host::diff_hooks_for;
use crate::writes::{announce, open_writes};
use crate::writes_tests::toasts;

const WAIT: Duration = Duration::from_secs(20);

/// The location of `path`.
fn place(path: &std::path::Path) -> Location {
    let text = path.to_str().expect("utf-8 path");
    Location::local(ProviderPath::from_text(text)).expect("location")
}

/// The change of the one hunk between the pair's sides, copied to the right.
fn the_change(hooks: &dyn DiffHooks, pair: PairRequest) -> CopyChange {
    let (sender, receiver) = mpsc::channel();
    let sink: DiffSink = Arc::new(move |update| {
        let _ = sender.send(update);
    });
    let _control = hooks.open_pair(pair.clone(), &CompareOptions::default(), sink);
    let DiffUpdate::Opened { text, .. } = receiver.recv_timeout(WAIT).expect("opened") else {
        panic!("expected Opened");
    };
    let text = text.expect("a text pair");
    let hunk = &text.hunks[0];
    CopyChange {
        pair,
        left_bytes: hunk.left_bytes.clone(),
        right_bytes: hunk.right_bytes.clone(),
        digest: hunk.digest.expect("digest"),
        direction: CopyDirection::ToRight,
        lines: None,
    }
}

/// With the journal's host, the production hooks copy into the real file and name the job.
#[test]
fn production_hooks_copy_through_the_journal_host() {
    let (profile, work) = (TempDir::new("hooks-profile"), TempDir::new("hooks-work"));
    let (left, right) = (work.path().join("l.txt"), work.path().join("r.txt"));
    fs::write(&left, "one\n").expect("left");
    fs::write(&right, "two\n").expect("right");
    let (host, _) = open_writes(Some(profile.path())).expect("journal opens");
    let hooks = diff_hooks_for(Some(host));
    let pair = PairRequest {
        id: 1,
        path: "r.txt".into(),
        left: Some(place(&left)),
        right: Some(place(&right)),
    };
    let change = the_change(hooks.as_ref(), pair);
    let (sender, receiver) = mpsc::channel();
    let sink: CopySink = Arc::new(move |done| {
        let _ = sender.send(done);
    });
    hooks.copy_change(change, sink);
    let done = receiver.recv_timeout(WAIT).expect("copy ends");
    assert_eq!(done.result, CopyResult::Copied);
    assert!(done.job.is_some(), "a committed copy names its job");
    assert_eq!(fs::read_to_string(&right).expect("right"), "one\n");
}

/// A start-up refusal reaches the shell's toasts.
#[gpui_kit::test]
fn announce_toasts_a_refusal(cx: &mut gpui_kit::TestAppContext) {
    let opened = scripted_shell(cx);
    cx.update(|cx| announce(Err("Copying is off: no folder".into()), cx));
    cx.run_until_parked();
    assert_eq!(toasts(cx, &opened), ["Copying is off: no folder"]);
}
