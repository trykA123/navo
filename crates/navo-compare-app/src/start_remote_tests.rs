//! Tests of the Start screen for remote Git sides: the probe, the chips and Compare (RG19).

use std::path::Path;

use gpui_kit::base::test_support::texts;
use gpui_kit::{AppContext as _, TestAppContext};
use navo_compare_cli::args::SideSpec;
use navo_compare_profile::recent::StartRows;

use crate::start::{SideHint, StartEvent};
use crate::start_git::probe;
use crate::start_tests::{TempDir, act, record, screen, settle};

const GOOD: &str = "https://github.com/owner/repo@main";
const CHIP_OK: &str = "Remote Git revision · downloaded when you compare";
const CHIP_BAD: &str = "Only https:// addresses without a user name are supported";

fn remote() -> SideSpec {
    SideSpec::GitUrl {
        url: "https://github.com/owner/repo".into(),
        rev: "main".into(),
    }
}

#[test]
fn probe_classifies_remote_addresses_without_the_network() {
    let cwd = Path::new(".");
    assert_eq!(probe(GOOD, cwd), SideHint::Remote(remote()));
    assert_eq!(probe("ssh://git@github.com/o/r", cwd), SideHint::BadAddress);
    assert_eq!(
        probe("https://user@github.com/o/r", cwd),
        SideHint::BadAddress
    );
}

/// Whether a visible chip with exactly `text` is drawn.
fn has_chip(cx: &mut TestAppContext, s: &crate::start_tests::Screen, text: &str) -> bool {
    cx.update_window(s.0, |_, window, _| {
        texts(window)
            .into_iter()
            .any(|e| e.visible() && e.text() == text)
    })
    .expect("window")
}

#[gpui_kit::test]
fn chips_show_the_remote_and_bad_address_texts(cx: &mut TestAppContext) {
    let dir = TempDir::new("remote-chip");
    let s = screen(cx, StartRows::default(), dir.path());
    settle(cx, &s, GOOD, "ssh://git@github.com/o/r");
    assert!(has_chip(cx, &s, CHIP_OK), "remote chip");
    assert!(has_chip(cx, &s, CHIP_BAD), "bad address chip");
}

#[gpui_kit::test]
fn two_remote_sides_enable_compare_and_emit_it(cx: &mut TestAppContext) {
    let dir = TempDir::new("remote-compare");
    let s = screen(cx, StartRows::default(), dir.path());
    let log = record(cx, &s.1);
    let hints = settle(cx, &s, GOOD, GOOD);
    assert_eq!(
        hints,
        (SideHint::Remote(remote()), SideHint::Remote(remote()))
    );
    act(cx, &s, |screen, window, cx| screen.submit(window, cx));
    let log = log.borrow();
    assert_eq!(log.len(), 1);
    assert!(matches!(
        &log[0],
        StartEvent::Compare { left, right, .. } if *left == remote() && *right == remote()
    ));
}
