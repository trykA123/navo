use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::{FerriteWindow, StartOptions, fake, open_options, press, settle, two_panes};
use crate::app_menu::{Identity, Install, QuickSettings};
use crate::browser::Operations;
use crate::settings::{Change, Values};
use crate::tokens::{self, Density, Mode, ThemeChoice};
use crate::window_view::{AppMenuHooks, SettingsHooks};

pub(super) fn values_of(settings: QuickSettings) -> Values {
    Values::default()
        .with(&Change::theme(settings.theme))
        .with(&Change::density(settings.density))
        .with(&Change::show_hidden(settings.show_hidden))
}

#[derive(Default)]
pub(super) struct FakeStore {
    pub stored: Mutex<Values>,
    pub changes: Mutex<Vec<Change>>,
    pub results: Mutex<Vec<QuickSettings>>,
    pub failure: Mutex<Option<String>>,
}

impl FakeStore {
    fn save(&self, changes: &[Change]) -> Result<Values, String> {
        self.changes
            .lock()
            .expect("changes")
            .extend_from_slice(changes);
        if let Some(message) = self.failure.lock().expect("failure").clone() {
            return Err(message);
        }
        let mut stored = self.stored.lock().expect("stored");
        *stored = changes
            .iter()
            .fold(stored.clone(), |all, one| all.with(one));
        self.results.lock().expect("results").push(stored.quick());
        Ok(stored.clone())
    }
}

pub(super) struct Rig {
    pub window: AnyWindowHandle,
    pub view: Entity<FerriteWindow>,
    pub store: Arc<FakeStore>,
    sessions: Rc<Cell<usize>>,
}

pub(super) fn rig(cx: &mut TestAppContext, theme: ThemeChoice) -> Rig {
    let settings = QuickSettings {
        theme,
        density: Density::Normal,
        show_hidden: true,
    };
    rig_with(cx, settings)
}

fn hidden_file(name: &str) -> rusty_fs::FsEntry {
    rusty_fs::FsEntry {
        name: rusty_core::ProviderPath::from_text(name),
        kind: rusty_fs::EntryKind::File,
        size: 1,
        modified_ms: None,
        attributes: rusty_fs::Attributes {
            hidden: true,
            ..rusty_fs::Attributes::default()
        },
    }
}

pub(super) fn rig_with(cx: &mut TestAppContext, settings: QuickSettings) -> Rig {
    rig_full(cx, settings, None)
}

pub(super) fn rig_full(
    cx: &mut TestAppContext,
    settings: QuickSettings,
    load_error: Option<&str>,
) -> Rig {
    let fs = fake();
    fs.add_entry("home", hidden_file("b-secret.txt"));
    let state = two_panes(&fs);
    let store = Arc::new(FakeStore {
        stored: Mutex::new(values_of(settings)),
        ..FakeStore::default()
    });
    let sessions: Rc<Cell<usize>> = Rc::default();
    let (fake_store, count) = (store.clone(), sessions.clone());
    let options = StartOptions {
        state,
        provider: fs,
        on_save: Rc::new(move |_| count.set(count.get() + 1)),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
        editor: super::super::EditorHooks::none(),
        files: super::super::FileHooks::none(),
        kinds: super::super::Kinds::builtin(),
        drives: super::super::DriveHooks::none(),
        app_menu: AppMenuHooks {
            identity: Identity {
                version: "0.0.1".into(),
                install: Install::Installed,
                folder: r"C:\Ferrite".into(),
            },
        },
        settings: SettingsHooks {
            values: values_of(settings),
            load_error: load_error.map(Into::into),
            save: Arc::new(move |changes| fake_store.save(changes)),
            ..SettingsHooks::none()
        },
        view: None,
        listing_cache: rusty_cache::ListingCache::default(),
        cache_keys: rusty_cache::CacheKeys::default(),
        shell_menu: None,
    };
    let (window, view) = open_options(cx, options);
    Rig {
        window,
        view,
        store,
        sessions,
    }
}

impl Rig {
    pub fn saved(&self) -> Vec<QuickSettings> {
        self.store.results.lock().expect("results").clone()
    }

    pub fn is_open(&self, cx: &mut TestAppContext) -> bool {
        self.view.read_with(cx, |view, _| view.app_menu.is_some())
    }

    pub fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    pub fn click(&self, cx: &mut TestAppContext, id: impl Into<gpui_kit::ElementId>) {
        let id = id.into();
        cx.update_window(self.window, |_, window, cx| window.click(id, cx))
            .expect("window");
        settle(cx, &self.view);
    }

    pub fn pane_has_focus(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| {
                let pane = view.browser.state().focused_pane();
                view.handle(pane, cx).is_focused(window)
            })
        })
        .expect("window")
    }

    pub fn pane_names(&self, cx: &mut TestAppContext) -> Vec<Vec<String>> {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            view.panes()
                .into_iter()
                .filter_map(|pane| state.pane(pane))
                .map(|pane| {
                    let listing = pane.active().listing();
                    listing
                        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
    }

    pub fn mode(&self, cx: &mut TestAppContext) -> Mode {
        cx.read(|cx| tokens::current(cx).mode)
    }

    fn button_expanded(&self, cx: &mut TestAppContext) -> Option<bool> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.find("title-bar-title").expanded()
        })
        .expect("window")
    }
}

#[gpui_kit::test]
fn the_app_button_opens_the_menu_and_takes_focus(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert_eq!(rig.button_expanded(cx), Some(false));
    rig.click(cx, "title-bar-title");
    assert!(rig.is_open(cx));
    assert_eq!(rig.button_expanded(cx), Some(true));
    assert!(!rig.pane_has_focus(cx));
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let identity = window.find("app-menu-identity");
        assert_eq!(
            identity.label(),
            Some(r"Ferrite 0.0.1, Installed · C:\Ferrite")
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn f10_opens_and_closes_the_menu(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    assert!(rig.is_open(cx));
    rig.press(cx, "f10");
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn escape_closes_the_menu_and_gives_focus_back_to_the_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert!(rig.pane_has_focus(cx));
    rig.press(cx, "f10");
    assert!(rig.is_open(cx));
    rig.press(cx, "down");
    rig.press(cx, "escape");
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn a_click_outside_closes_the_menu_and_restores_focus(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.click(cx, "title-bar-title");
    rig.click(cx, "app-menu-backdrop");
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn the_theme_tile_applies_and_saves_each_choice(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert_eq!(rig.mode(cx), Mode::Light);
    rig.press(cx, "f10");
    rig.press(cx, "enter");
    assert_eq!(rig.mode(cx), Mode::Light);
    rig.press(cx, "enter");
    assert_eq!(rig.mode(cx), Mode::Dark);
    rig.click(cx, ("app-menu-tile", 0usize));
    assert_eq!(rig.mode(cx), Mode::Light);
    let themes: Vec<ThemeChoice> = rig.saved().iter().map(|s| s.theme).collect();
    assert_eq!(
        themes,
        [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::System]
    );
    assert!(rig.is_open(cx), "a tile keeps the menu open");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(("app-menu-tile", 0usize)).value(),
            Some("System")
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_saved_theme_applies_at_start(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::Dark);
    assert_eq!(rig.mode(cx), Mode::Dark);
    assert!(rig.saved().is_empty());
}

#[gpui_kit::test]
fn unavailable_rows_neither_act_nor_close_the_menu(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    for key in ["c", "n", "w", "h"] {
        rig.press(cx, key);
        rig.press(cx, "enter");
        assert!(rig.is_open(cx), "{key} closed the menu");
    }
    rig.click(cx, ("app-menu-row", 3usize));
    assert!(rig.is_open(cx));
    assert!(rig.saved().is_empty());
    assert_eq!(rig.sessions.get(), 0);
}

#[gpui_kit::test]
fn quit_saves_the_session(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    rig.press(cx, "q");
    rig.press(cx, "enter");
    assert_eq!(rig.sessions.get(), 1);
}

#[gpui_kit::test]
fn alt_space_is_left_to_the_system_menu(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "alt-space");
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn f10_does_nothing_while_a_prompt_is_open(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f7");
    assert!(rig.view.read_with(cx, |view, _| view.prompt.is_some()));
    rig.press(cx, "f10");
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn f10_does_nothing_while_a_confirm_dialog_is_open(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "down");
    rig.press(cx, "f8");
    assert!(
        rig.view
            .read_with(cx, |view, _| view.browser.pending().is_some())
    );
    rig.press(cx, "f10");
    assert!(!rig.is_open(cx));
}

#[test]
fn the_f10_cap_reads_menu_and_f9_stays_the_drawer() {
    let keys = crate::window_view::layer_keys(None);
    assert_eq!(keys[8], ("F9", "Drawer"));
    assert_eq!(keys[9], ("F10", "Menu"));
}

#[gpui_kit::test]
fn the_menu_shows_theme_density_and_hidden_tiles(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let tiles: Vec<(String, String)> = (0..3usize)
            .map(|index| {
                let tile = window.find(("app-menu-tile", index));
                (
                    tile.label().unwrap_or_default().to_owned(),
                    tile.value().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let expected = [
            ("Theme", "System"),
            ("Density", "Normal"),
            ("Hidden files", "Shown"),
        ]
        .map(|(label, value)| (label.to_owned(), value.to_owned()));
        assert_eq!(tiles, expected);
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_density_tile_changes_the_rows_and_saves(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    rig.click(cx, ("app-menu-tile", 1usize));
    assert_eq!(
        cx.read(|cx| tokens::current(cx).density),
        Density::Comfortable
    );
    rig.click(cx, ("app-menu-tile", 1usize));
    assert_eq!(cx.read(|cx| tokens::current(cx).density), Density::Compact);
    let saved: Vec<Density> = rig.saved().iter().map(|s| s.density).collect();
    assert_eq!(saved, [Density::Comfortable, Density::Compact]);
    assert_eq!(rig.mode(cx), Mode::Light, "density keeps the theme");
}

#[gpui_kit::test]
fn the_hidden_tile_hides_hidden_files_in_both_panes_and_saves(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert!(
        rig.pane_names(cx)
            .iter()
            .all(|names| names.contains(&"b-secret.txt".to_owned()))
    );
    rig.press(cx, "f10");
    rig.click(cx, ("app-menu-tile", 2usize));
    let panes = rig.pane_names(cx);
    assert_eq!(panes.len(), 2);
    assert!(
        panes
            .iter()
            .all(|names| !names.contains(&"b-secret.txt".to_owned()))
    );
    assert!(
        panes
            .iter()
            .all(|names| names.contains(&"a.txt".to_owned()))
    );
    let saved = rig.saved().last().map(|s| s.show_hidden);
    assert_eq!(saved, Some(false));
}

#[gpui_kit::test]
fn saved_density_and_hidden_choices_apply_at_start(cx: &mut TestAppContext) {
    let settings = QuickSettings {
        theme: ThemeChoice::System,
        density: Density::Compact,
        show_hidden: false,
    };
    let rig = rig_with(cx, settings);
    assert_eq!(cx.read(|cx| tokens::current(cx).density), Density::Compact);
    assert!(
        rig.pane_names(cx)
            .iter()
            .all(|names| !names.contains(&"b-secret.txt".to_owned()))
    );
}
