//! Tests for the compare host: key binding count, the compare file reads and opening text and Git pairs.

use std::fs;
use std::sync::Arc;

use sidewise_git::fixture::Fixture;
use sidewise_git::location::{Form, encode};
use sidewise_ui::browser::Waker;
use sidewise_ui::compare_diff::{CompareOptions, DiffSink, DiffUpdate, PairRequest};
use sidewise_ui::compare_keys::compare_keys;

use navo_compare_profile::appearance::{LayoutPref, UiPrefs, ViewPresetPref};
use sidewise_ui::compare_diff::Layout;
use sidewise_ui::window_view::{LookDefaults, LookPreset};

use super::compare_host::{
    action_name, bind_compare_keys, diff_hooks, look_of, new_compare_screen,
};
use super::compare_run::location_of;
use super::host_parts::compare_files;

/// Every compare key that names a registered action binds, plus Ctrl+Z in the two compare
/// contexts; the count matches those keys.
#[gpui_kit::test]
fn bind_compare_keys_binds_every_compare_key(cx: &mut gpui_kit::TestAppContext) {
    let (added, expected, failed_compare) = cx.update(|cx| {
        gpui_kit::init(cx);
        let added = bind_compare_keys(cx);
        let mut expected = 2;
        let mut failed_compare = Vec::new();
        for entry in compare_keys().expect("the compare key file parses") {
            let builds = action_name(&entry.command)
                .is_some_and(|name| cx.build_action(&name, None).is_ok());
            if builds {
                expected += entry.keys.len();
            } else if entry.command.starts_with("compare.") {
                failed_compare.push(entry.command);
            }
        }
        (added, expected, failed_compare)
    });
    assert_eq!(added, expected);
    assert!(
        failed_compare.is_empty(),
        "compare commands that do not build: {failed_compare:?}"
    );
    assert!(expected > 0);
}

/// A command name becomes a PascalCase action name after its namespace.
#[test]
fn action_name_pascal_cases_the_command() {
    assert_eq!(
        action_name("compare.next_difference"),
        Some("compare::NextDifference".to_owned())
    );
    assert_eq!(
        action_name("viewer.page_up"),
        Some("viewer::PageUp".to_owned())
    );
    assert_eq!(action_name("nodot"), None);
}

/// The compare files read a local file as bytes up to a limit and open it as a paged source.
#[test]
fn compare_files_read_local_files() {
    let path = std::env::temp_dir().join(format!("sidewise-host-{}.txt", std::process::id()));
    fs::write(&path, b"hello").expect("write temp file");
    let loc = location_of(&path).expect("location");
    let files = compare_files();
    let bytes = (files.bytes)(&loc, 64).expect("read bytes");
    let too_large = (files.bytes)(&loc, 2).expect_err("limit rejects the file");
    let source = (files.source)(&loc).expect("open source");
    let _ = fs::remove_file(&path);
    assert_eq!(bytes, b"hello");
    assert!(too_large.contains("Too large"), "message: {too_large}");
    assert_eq!(source.len(), 5);
}

/// A text pair opens in the file diff.
#[gpui_kit::test]
fn compare_screen_opens_a_text_pair(cx: &mut gpui_kit::TestAppContext) {
    let dir = std::env::temp_dir().join(format!("sidewise-host-pair-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("a.txt"), b"one\n").expect("write left");
    fs::write(dir.join("b.txt"), b"two\n").expect("write right");
    let wake: Waker = Arc::new(|| {});
    let (window, screen) =
        crate::test_window::open(cx, |_, _| new_compare_screen(wake, LookDefaults::default()));
    let request = PairRequest {
        id: 0,
        path: "a.txt".into(),
        left: Some(location_of(&dir.join("a.txt")).expect("left")),
        right: Some(location_of(&dir.join("b.txt")).expect("right")),
    };
    let opened = cx.update(|cx| {
        window
            .update(cx, |_, window, cx| {
                screen.update(cx, |screen, cx| {
                    screen.show_pair((request, None), window, cx)
                })
            })
            .expect("update window")
    });
    let open = screen.read_with(cx, |screen, _| screen.is_file_open());
    let _ = fs::remove_dir_all(&dir);
    assert!(opened);
    assert!(open);
}

/// The diff hooks open a pair of Git files and report `Opened`.
#[test]
fn diff_hooks_open_a_git_pair() {
    let fixture = Fixture::new();
    let _ = fixture.commit("main", &[("a.txt", b"one\n")]);
    let root = encode(fixture.path(), "main", Form::Stored).expect("encode");
    let text = format!("{}/a.txt", root.path().to_text());
    let place = || {
        sidewise_core::Location::with_provider(
            root.provider().clone(),
            sidewise_core::ProviderPath::from_text(&text),
        )
        .expect("location")
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    let sink: DiffSink = Arc::new(move |update| drop(sender.send(update)));
    let request = PairRequest {
        id: 0,
        path: "a.txt".into(),
        left: Some(place()),
        right: Some(place()),
    };
    let _control = diff_hooks().open_pair(request, &CompareOptions::default(), sink);
    let update = receiver.recv_timeout(std::time::Duration::from_secs(10));
    assert!(
        matches!(update, Ok(DiffUpdate::Opened { text: Some(_), .. })),
        "{update:?}"
    );
}

/// `look_of` maps each preset one to one, and words, wrap and layout follow the prefs.
#[test]
fn look_of_maps_presets_and_parts() {
    for (pref, want) in [
        (ViewPresetPref::Simple, LookPreset::Simple),
        (ViewPresetPref::Rich, LookPreset::Rich),
        (ViewPresetPref::Custom, LookPreset::Custom),
    ] {
        let prefs = UiPrefs {
            preset: pref,
            words: false,
            wrap_lines: true,
            layout: LayoutPref::Unified,
            ..UiPrefs::default()
        };
        let look = look_of(&prefs);
        assert_eq!(look.preset, want);
        assert!(!look.words && look.wrap);
        assert_eq!(look.layout, Some(Layout::Unified));
    }
    assert_eq!(look_of(&UiPrefs::default()).layout, None);
}

/// A screen made with a look reports it.
#[gpui_kit::test]
fn new_compare_screen_starts_with_the_given_look(cx: &mut gpui_kit::TestAppContext) {
    let look = LookDefaults {
        preset: LookPreset::Simple,
        ..LookDefaults::default()
    };
    let wake: Waker = Arc::new(|| {});
    let (_, screen) = crate::test_window::open(cx, |_, _| new_compare_screen(wake, look));
    assert_eq!(screen.read_with(cx, |s, _| s.look_defaults()), look);
}
