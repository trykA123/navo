use std::time::Duration;

use gpui_kit::TestAppContext;

use super::actions::{Action, items};
use super::secret::Facts;
use super::tests::{Call, Fake, Opened, SECRET, history, open, row};
use super::{ClipId, ClipKind, ClipMode, ClipPaletteEvent, ClipRow, How, Origin};

fn secret(id: u64, user: &str) -> ClipRow {
    let mut secret = row(id, ClipKind::Secret, "Build server", Origin::Saved);
    secret.detail = user.into();
    secret
}

fn vault() -> Vec<ClipRow> {
    vec![secret(7, "admin"), secret(8, "")]
}

fn calls(fake: &Fake) -> Vec<Call> {
    std::mem::take(&mut *fake.calls.lock().expect("calls"))
}

fn shown(palette: &Opened, cx: &mut TestAppContext) -> Option<String> {
    palette.palette.read_with(cx, |palette, _| {
        let row = palette.state.selected_row()?;
        palette.shown_secret(row).map(str::to_owned)
    })
}

fn buffer_is_empty(palette: &Opened, cx: &mut TestAppContext) -> bool {
    palette
        .palette
        .read_with(cx, |palette, _| palette.overlay.revealed.is_none())
}

fn tick(palette: &Opened, seconds: u64, cx: &mut TestAppContext) {
    cx.executor().advance_clock(Duration::from_secs(seconds));
    palette.settle(cx);
}

fn status(palette: &Opened, cx: &mut TestAppContext) -> Option<String> {
    palette.label("clip-palette-status", cx)
}

fn clears(palette: &Opened, cx: &mut TestAppContext) -> Option<String> {
    palette.label("clip-palette-status-clears", cx)
}

#[gpui_kit::test]
fn alt_r_shows_the_secret_and_moving_the_selection_hides_it(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], vault());
    let palette = open(cx, fake.clone());
    assert_eq!(shown(&palette, cx), None);
    palette.press("alt-r", cx);
    assert_eq!(calls(&fake), [Call::Reveal(ClipId(7))]);
    assert_eq!(shown(&palette, cx).as_deref(), Some(SECRET));
    palette.press("down", cx);
    assert!(buffer_is_empty(&palette, cx));
    palette.press("up", cx);
    assert_eq!(shown(&palette, cx), None);
}

#[gpui_kit::test]
fn alt_r_again_hides_and_a_plain_row_ignores_it(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], vault());
    let palette = open(cx, fake.clone());
    palette.press("alt-r", cx);
    palette.press("alt-r", cx);
    assert!(buffer_is_empty(&palette, cx));
    let plain = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, plain.clone());
    palette.press("alt-r", cx);
    assert!(calls(&plain).is_empty());
}

#[gpui_kit::test]
fn closing_the_palette_hides_the_secret(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], vault());
    let palette = open(cx, fake);
    palette.press("alt-r", cx);
    palette.press("escape", cx);
    assert!(buffer_is_empty(&palette, cx));
    assert_eq!(*palette.events.borrow(), [ClipPaletteEvent::Dismissed]);
}

#[gpui_kit::test]
fn a_refresh_that_drops_the_row_hides_the_secret(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], vault());
    let palette = open(cx, fake.clone());
    palette.press("alt-r", cx);
    fake.change(vec![secret(8, "")], 2);
    palette.settle(cx);
    assert!(buffer_is_empty(&palette, cx));
}

#[gpui_kit::test]
fn enter_on_a_secret_counts_down_from_30_and_ends_cleared(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], vault());
    let palette = open(cx, fake.clone());
    palette.press("enter", cx);
    assert_eq!(calls(&fake), [Call::Copy(ClipId(7), How::Copy)]);
    assert_eq!(
        status(&palette, cx).as_deref(),
        Some("Copied without showing")
    );
    assert_eq!(clears(&palette, cx).as_deref(), Some("Clears in 30 s"));
    tick(&palette, 1, cx);
    assert_eq!(clears(&palette, cx).as_deref(), Some("Clears in 29 s"));
    tick(&palette, 28, cx);
    assert_eq!(clears(&palette, cx).as_deref(), Some("Clears in 1 s"));
    tick(&palette, 1, cx);
    assert_eq!(status(&palette, cx).as_deref(), Some("Clipboard cleared"));
    assert_eq!(clears(&palette, cx), None);
    tick(&palette, 3, cx);
    assert_eq!(status(&palette, cx), None);
}

#[gpui_kit::test]
fn a_newer_status_stops_the_countdown(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], vault());
    let palette = open(cx, fake);
    palette.press("enter", cx);
    palette.press("alt-u", cx);
    tick(&palette, 2, cx);
    assert_eq!(clears(&palette, cx).as_deref(), Some("Clears in 28 s"));
    palette.press("delete", cx);
    assert_eq!(status(&palette, cx).as_deref(), Some("Deleted"));
    tick(&palette, 40, cx);
    assert_eq!(status(&palette, cx), None);
}

#[gpui_kit::test]
fn alt_u_copies_the_user_name_when_the_row_has_one(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], vault());
    let palette = open(cx, fake.clone());
    palette.press("alt-u", cx);
    assert_eq!(calls(&fake), [Call::CopyUser(ClipId(7))]);
    assert_eq!(status(&palette, cx).as_deref(), Some("Copied user name"));
    palette.press("down", cx);
    palette.press("alt-u", cx);
    assert!(calls(&fake).is_empty());
}

#[gpui_kit::test]
fn ctrl_s_saves_a_history_row_and_the_saved_row_appears(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake.clone());
    palette.press("ctrl-s", cx);
    assert_eq!(calls(&fake), [Call::SaveFromHistory(ClipId(1))]);
    assert!(palette.titles(cx).contains(&"saved copy".to_owned()));
    assert_eq!(status(&palette, cx).as_deref(), Some("Saved as snippet"));
    palette.press("down", cx);
    palette.press("ctrl-s", cx);
    assert_eq!(calls(&fake), [Call::SaveFromHistory(ClipId(2))]);
    assert_eq!(status(&palette, cx).as_deref(), Some("Saved as path item"));
}

#[gpui_kit::test]
fn ctrl_s_ignores_a_saved_row(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], vault());
    let palette = open(cx, fake.clone());
    palette.press("ctrl-s", cx);
    assert!(calls(&fake).is_empty());
}

#[test]
fn the_menu_lists_show_or_hide_and_the_user_name_only_when_there_is_one() {
    let secret = secret(7, "admin");
    let actions = |facts| -> Vec<Action> {
        items(&secret, facts)
            .iter()
            .map(|item| item.action)
            .collect()
    };
    let with_user = Facts {
        is_shown: false,
        has_user: true,
        ..Facts::default()
    };
    assert_eq!(
        actions(with_user),
        [
            Action::Enter,
            Action::Reveal,
            Action::CopyUser,
            Action::Delete,
            Action::Manage
        ]
    );
    assert_eq!(
        actions(Facts::default()),
        [
            Action::Enter,
            Action::Reveal,
            Action::Delete,
            Action::Manage
        ]
    );
    let labels = |facts| items(&secret, facts)[1].label.clone();
    assert_eq!(labels(with_user), "Show");
    let shown = Facts {
        is_shown: true,
        ..with_user
    };
    assert_eq!(labels(shown), "Hide");
}
