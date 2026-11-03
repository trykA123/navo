use std::sync::atomic::Ordering;

use gpui_kit::TestAppContext;

use super::ClipMode;
use super::tests::{Fake, history, open, open_in};
use super::view::Place;

const LINE: &str = "clip-palette-intro";

fn due() -> std::sync::Arc<Fake> {
    let fake = Fake::new(vec![ClipMode::All, ClipMode::Recent], history());
    fake.intro.store(true, Ordering::SeqCst);
    fake
}

#[gpui_kit::test]
fn the_first_open_shows_the_line_and_the_next_does_not(cx: &mut TestAppContext) {
    let fake = due();
    let first = open(cx, fake.clone());
    assert_eq!(
        first.label(LINE, cx),
        Some(navo_l10n::tr("clip-intro")),
        "the first palette says what is kept"
    );
    let second = open(cx, fake);
    assert_eq!(second.label(LINE, cx), None);
}

#[gpui_kit::test]
fn the_line_is_the_wording_the_owner_chose(cx: &mut TestAppContext) {
    let first = open_in(cx, due(), Place::Anywhere);
    assert_eq!(
        first.label(LINE, cx).as_deref(),
        Some("Navo keeps what you copy on this PC. Settings \u{203a} Clipboard turns it off.")
    );
}

#[gpui_kit::test]
fn a_palette_without_a_due_line_shows_none(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    assert_eq!(open(cx, fake).label(LINE, cx), None);
}

#[gpui_kit::test]
fn only_the_saved_and_secrets_empty_states_advertise_ctrl_n(cx: &mut TestAppContext) {
    let modes = vec![
        ClipMode::All,
        ClipMode::Recent,
        ClipMode::Saved,
        ClipMode::Secrets,
        ClipMode::Notes,
    ];
    let palette = open(cx, Fake::new(modes.clone(), Vec::new()));
    for mode in modes {
        let keys = palette
            .palette
            .read_with(cx, |palette, _| palette.says(mode, false).keys);
        let offers_new = keys.iter().any(|(key, _)| *key == "ctrl-n");
        let wants_new = matches!(mode, ClipMode::Saved | ClipMode::Secrets);
        assert_eq!(offers_new, wants_new, "{mode:?}: {keys:?}");
    }
}
