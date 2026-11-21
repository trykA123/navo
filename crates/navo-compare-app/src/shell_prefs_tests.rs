//! Tests of the View preferences global: reading the profile, saving back and the defaults.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::{TestAppContext, WindowAppearance};
use navo_compare_profile::appearance::{AppearanceMode, PaletteId, UiPrefs};
use navo_compare_profile::brand::SLUG;
use navo_compare_profile::paths::open_profile;

use sidewise_kit::tokens::Mode;

use crate::shell::Shell;
use crate::shell_prefs::{install_prefs, prefs, save_prefs, theme_choice};
use crate::test_window::open;
use crate::theme::SidewiseTheme;

static COUNT: AtomicUsize = AtomicUsize::new(0);

/// A temporary folder for one profile, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    /// Makes an empty folder named after the test and a counter.
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{SLUG}-vm2-{}-{name}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("create temp folder");
        Self(path)
    }

    /// The folder path.
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    /// Removes the folder and everything in it.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The settings of the profile on disk are read into the installed global.
#[gpui_kit::test]
fn prefs_read_the_profile_settings(cx: &mut TestAppContext) {
    let temp = TempDir::new("read");
    let profile = open_profile(Some(temp.path())).expect("open profile");
    fs::write(profile.settings_file(), "[ui]\nwords = false\n").expect("write settings");
    cx.update(|cx| install_prefs(Some(temp.path()), cx))
        .expect("install");
    assert!(!cx.update(|cx| prefs(cx)).words, "words = false comes back");
}

/// Saved preferences are written to the profile and survive a reinstall.
#[gpui_kit::test]
fn saved_prefs_survive_a_reinstall(cx: &mut TestAppContext) {
    let temp = TempDir::new("save");
    cx.update(|cx| install_prefs(Some(temp.path()), cx))
        .expect("install");
    let wanted = UiPrefs {
        words: false,
        wrap_lines: true,
        ..UiPrefs::default()
    };
    cx.update(|cx| save_prefs(wanted.clone(), cx))
        .expect("save");
    cx.update(|cx| install_prefs(Some(temp.path()), cx))
        .expect("reinstall");
    assert_eq!(
        cx.update(|cx| prefs(cx)),
        wanted,
        "the saved preferences come back"
    );
}

/// Without an installed global the preferences are the defaults.
#[gpui_kit::test]
fn missing_global_gives_the_defaults(cx: &mut TestAppContext) {
    assert_eq!(cx.update(|cx| prefs(cx)), UiPrefs::default());
}

/// With no preferences global there is no saved theme choice.
#[gpui_kit::test]
fn theme_choice_is_none_without_prefs(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert_eq!(theme_choice(WindowAppearance::Dark, cx), None);
    });
}

/// System follows the OS appearance; Light and Dark pin the mode whatever the OS says.
#[gpui_kit::test]
fn system_follows_the_os_and_light_and_dark_pin(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let cases = [
            (AppearanceMode::System, WindowAppearance::Light, Mode::Light),
            (AppearanceMode::System, WindowAppearance::Dark, Mode::Dark),
            (AppearanceMode::Light, WindowAppearance::Light, Mode::Light),
            (AppearanceMode::Light, WindowAppearance::Dark, Mode::Light),
            (AppearanceMode::Dark, WindowAppearance::Light, Mode::Dark),
            (AppearanceMode::Dark, WindowAppearance::Dark, Mode::Dark),
        ];
        for (mode, os, wanted) in cases {
            let saved = UiPrefs {
                palette: PaletteId::Ember,
                mode,
                ..UiPrefs::default()
            };
            save_prefs(saved, cx).expect("save");
            assert_eq!(
                theme_choice(os, cx),
                Some((PaletteId::Ember, wanted)),
                "{mode:?} under {os:?}"
            );
        }
    });
}

/// A new shell opens in the saved palette and mode, and an OS change keeps them.
#[gpui_kit::test]
fn a_new_shell_opens_in_the_saved_theme(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let saved = UiPrefs {
            palette: PaletteId::Ember,
            mode: AppearanceMode::Dark,
            ..UiPrefs::default()
        };
        save_prefs(saved, cx).expect("save");
    });
    let _window = open(cx, Shell::new);
    cx.update(|cx| {
        let theme = *cx.global::<SidewiseTheme>();
        assert_eq!((theme.palette, theme.mode), (PaletteId::Ember, Mode::Dark));
        Shell::apply_appearance(WindowAppearance::Light, cx);
        let theme = *cx.global::<SidewiseTheme>();
        assert_eq!((theme.palette, theme.mode), (PaletteId::Ember, Mode::Dark));
    });
}
