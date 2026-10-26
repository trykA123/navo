use rusty_core::{DropKeys, DropOp, Location, Step, drop_op};

use super::*;
use crate::browser::{DropConfirm, DropRequest, PasteRefusal, drop_refusal, same_volume};

fn request(fs: &FakeFs, kind: OpKind, confirm: DropConfirm) -> DropRequest {
    DropRequest {
        kind,
        sources: vec![fs.location("home/docs")],
        target: fs.location("dest"),
        confirm,
    }
}

#[test]
fn a_drop_into_another_folder_moves_on_the_same_volume() {
    let fs = fake();
    let mut browser = browser(&fs, AppState::new(fs.location("home")));
    let record = Arc::new(Mutex::new(Record::default()));
    let shared = Arc::clone(&record);
    browser.set_operations(Operations {
        retain_deletes: None,
        write: None,
        log: Arc::new(move |plan| {
            assert_eq!(plan.kind, OpKind::Move);
            assert!(matches!(plan.steps.as_slice(), [Step::Rename { .. }]));
            Ok(Box::new(SharedLog(Arc::clone(&shared))) as Box<dyn OpLog>)
        }),
        protected: Vec::new(),
    });
    let request = request(&fs, OpKind::Move, DropConfirm::Never);
    assert_eq!(
        drop_op(
            same_volume(&request.sources[0], &request.target),
            DropKeys::default()
        ),
        DropOp::Move
    );
    let id = browser.drop_into(request).unwrap();
    settle(&mut browser);
    assert!(browser.pending().is_none());
    assert_eq!(browser.jobs()[0].id, id);
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    assert!(exists(&fs, "dest/docs/report.txt"));
    assert!(!exists(&fs, "home/docs"));
    let record = record.lock().unwrap();
    assert_eq!(record.steps.len(), 1);
    assert_eq!(record.finished, Some(JobOutcome::Done));
}

#[test]
fn a_copy_drop_keeps_the_sources() {
    let fs = fake();
    let mut browser = browser(&fs, AppState::new(fs.location("home")));
    browser
        .drop_into(request(&fs, OpKind::Copy, DropConfirm::Never))
        .unwrap();
    settle(&mut browser);
    assert!(browser.pending().is_none());
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    assert!(exists(&fs, "home/docs/report.txt"));
    assert!(exists(&fs, "dest/docs/report.txt"));
}

#[test]
fn a_drop_refuses_empty_sources_its_parent_and_its_own_subtree() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    let cases = [
        (Vec::new(), "dest", PasteRefusal::Nothing),
        (
            vec![fs.location("home/docs")],
            "home",
            PasteRefusal::AlreadyHere,
        ),
        (
            vec![fs.location("home/docs")],
            "home/docs",
            PasteRefusal::IntoItself,
        ),
        (
            vec![fs.location("home/docs")],
            "home/docs/child",
            PasteRefusal::IntoItself,
        ),
    ];
    for (sources, target, refusal) in cases {
        let request = DropRequest {
            sources,
            target: fs.location(target),
            ..request(&fs, OpKind::Move, DropConfirm::Never)
        };
        assert_eq!(browser.drop_into(request), Err(refusal));
    }
    settle(&mut browser);
    assert!(browser.pending().is_none());
    assert!(browser.jobs().is_empty());
    assert!(exists(&fs, "home/docs/report.txt"));
}

#[test]
fn moves_confirmation_asks_for_move() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser
        .drop_into(request(&fs, OpKind::Move, DropConfirm::Moves))
        .unwrap();
    settle(&mut browser);
    assert_eq!(browser.pending().unwrap().plan.kind, OpKind::Move);
    assert!(browser.jobs().is_empty());
    assert!(exists(&fs, "home/docs/report.txt"));
    assert!(!exists(&fs, "dest/docs"));
    assert!(browser.confirm());
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
}

#[test]
fn moves_confirmation_does_not_ask_for_copy() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser
        .drop_into(request(&fs, OpKind::Copy, DropConfirm::Moves))
        .unwrap();
    settle(&mut browser);
    assert!(browser.pending().is_none());
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    assert!(exists(&fs, "dest/docs/report.txt"));
}

#[test]
fn always_confirmation_asks_for_copy_and_move() {
    for kind in [OpKind::Copy, OpKind::Move] {
        let fs = fake();
        let mut browser = browser(&fs, home_beside_dest(&fs));
        browser
            .drop_into(request(&fs, kind, DropConfirm::Always))
            .unwrap();
        settle(&mut browser);
        assert_eq!(browser.pending().unwrap().plan.kind, kind);
        assert!(browser.jobs().is_empty());
        assert!(!exists(&fs, "dest/docs"));
    }
}

#[test]
fn protected_drop_sources_and_targets_always_ask() {
    for protected in ["home/docs", "dest"] {
        for kind in [OpKind::Copy, OpKind::Move] {
            let fs = fake();
            let mut browser = browser(&fs, home_beside_dest(&fs));
            browser.set_operations(Operations {
                protected: vec![fs.location(protected)],
                ..Operations::unlogged()
            });
            browser
                .drop_into(request(&fs, kind, DropConfirm::Never))
                .unwrap();
            settle(&mut browser);
            assert!(browser.pending().is_some(), "{protected}: {kind:?}");
            assert!(browser.jobs().is_empty());
            assert!(!exists(&fs, "dest/docs"));
        }
    }
}

#[test]
fn a_drop_uses_the_source_tab_in_another_pane() {
    let fs = fake();
    let mut state = home_beside_dest(&fs);
    let source_tab = state.active_tab().id();
    apply(
        &mut state,
        &Command::LayoutFocus {
            direction: Direction::Right,
        },
    )
    .unwrap();
    assert_ne!(state.active_tab().id(), source_tab);
    let mut browser = browser(&fs, state);
    browser
        .drop_into(request(&fs, OpKind::Copy, DropConfirm::Never))
        .unwrap();
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].source_tab, source_tab);
}

#[test]
fn a_drop_uses_an_inactive_source_tab_in_another_window() {
    let fs = fake();
    let mut state = AppState::new(fs.location("home"));
    let source_tab = state.active_tab().id();
    apply(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(fs.location("dest")),
        },
    )
    .unwrap();
    apply(
        &mut state,
        &Command::WindowOpen {
            location: Some(fs.location("dest")),
        },
    )
    .unwrap();
    let mut browser = browser(&fs, state);
    browser
        .drop_into(request(&fs, OpKind::Copy, DropConfirm::Always))
        .unwrap();
    settle(&mut browser);
    assert_eq!(browser.pending().unwrap().source_tab, source_tab);
    browser.confirm();
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].source_tab, source_tab);
}

#[test]
fn a_drop_without_an_open_source_folder_uses_the_active_tab() {
    let fs = fake();
    let mut browser = browser(&fs, AppState::new(fs.location("dest")));
    let active_tab = browser.state().active_tab().id();
    browser
        .drop_into(request(&fs, OpKind::Copy, DropConfirm::Always))
        .unwrap();
    settle(&mut browser);
    assert_eq!(browser.pending().unwrap().source_tab, active_tab);
}

#[test]
fn same_volume_compares_drive_share_and_provider_volumes() {
    for (a, b, expected) in [
        (r"C:\home\a.txt", r"c:\dest", true),
        (r"C:\home\a.txt", r"D:\dest", false),
        (r"\\srv\share\a.txt", r"\\SRV\SHARE\dest", true),
        (r"\\srv\share\a.txt", r"\\srv\other\dest", false),
        ("fake://home/a.txt", "fake://dest", true),
        ("fake://home/a.txt", "other://dest", false),
        ("fake://home/a.txt", "/dest", false),
    ] {
        assert_eq!(
            same_volume(&Location::new(a).unwrap(), &Location::new(b).unwrap()),
            expected
        );
    }
}

#[test]
fn paste_copies_or_moves_into_the_active_folder_without_confirmation() {
    for kind in [OpKind::Copy, OpKind::Move] {
        let fs = fake();
        let mut browser = browser(&fs, AppState::new(fs.location("dest")));
        browser.paste(kind, vec![fs.location("home/docs")]).unwrap();
        settle(&mut browser);
        assert!(browser.pending().is_none());
        assert_eq!(browser.jobs()[0].kind, kind);
        assert_eq!(browser.jobs()[0].status, JobStatus::Done);
        assert!(exists(&fs, "dest/docs/report.txt"));
        assert_eq!(exists(&fs, "home/docs"), kind == OpKind::Copy);
    }
}

#[test]
fn refusals_ignore_case_on_windows_paths_like_the_planner() {
    let loc = |text: &str| Location::new(text).unwrap();
    let sources = [loc(r"C:\Work\Proj")];
    let cases = [
        (r"c:\work", PasteRefusal::AlreadyHere),
        (r"c:\WORK\proj", PasteRefusal::IntoItself),
        (r"c:\work\PROJ\sub", PasteRefusal::IntoItself),
    ];
    for (target, refusal) in cases {
        assert_eq!(
            drop_refusal(&loc(target), &sources),
            Some(refusal),
            "{target}"
        );
    }
    assert_eq!(drop_refusal(&loc(r"c:\other"), &sources), None);
}
