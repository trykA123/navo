use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, ParentElement, Render, Styled,
    TestAppContext, Window, div, px,
};
use navo_core::Location;

use super::age::age;
use super::preview::file_list;
use super::state::{Entry, PaletteState};
use super::{
    ClipCounts, ClipId, ClipKind, ClipMode, ClipPalette, ClipPaletteEvent, ClipPaletteProps,
    ClipResults, ClipRow, ClipboardHooks, FolderProbe, How, Origin, Place, SavedDraft, SavedEdit,
    Scope, SecretText,
};
use crate::test_window::open_sized;

const NOW: i64 = 1_800_000_000_000;
const MINUTE: i64 = 60_000;
pub(super) const SECRET: &str = "hunter2-do-not-show";

pub(super) fn row(id: u64, kind: ClipKind, title: &str, from: Origin) -> ClipRow {
    ClipRow {
        id: ClipId(id),
        kind,
        title: title.into(),
        detail: String::new(),
        source_app: Some("Notepad".into()),
        when_ms: NOW - MINUTE * id as i64,
        scope: Scope::Everywhere,
        from,
        ranges: Vec::new(),
    }
}

fn results(rows: Vec<ClipRow>, revision: u64) -> ClipResults {
    ClipResults {
        kept: rows.len() as u32,
        rows,
        revision,
        notice: None,
    }
}

/// A draft as the hooks saw it, with the text in the clear for the assertions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Draft {
    pub kind: ClipKind,
    pub title: String,
    pub body: String,
    pub user: Option<String>,
    pub scope: Scope,
}

impl Draft {
    fn of(draft: &SavedDraft) -> Self {
        Self {
            kind: draft.kind,
            title: draft.title.clone(),
            body: draft.body.expose().to_owned(),
            user: draft.user.clone(),
            scope: draft.scope.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Call {
    Copy(ClipId, How),
    Delete(ClipId),
    Reveal(ClipId),
    CopyUser(ClipId),
    SaveFromHistory(ClipId),
    Save(Draft),
    Update(ClipId, Draft),
    RenameNote(Scope, Scope),
}

pub(super) struct Fake {
    modes: Vec<ClipMode>,
    pub calls: Mutex<Vec<Call>>,
    /// Makes `copy` fail, as a write the clipboard refused.
    pub refuse_copy: std::sync::atomic::AtomicBool,
    /// The first-open line is due until a palette takes it.
    pub intro: std::sync::atomic::AtomicBool,
    rows: Mutex<Vec<ClipRow>>,
    /// The stored text of each note by scope, and every `set_note` call in order.
    pub notes: Mutex<Vec<(Scope, String)>>,
    /// What `saved_for_edit` gives back, by id.
    pub edits: Mutex<Vec<(ClipId, Draft)>>,
    /// Makes `save` and `update_saved` refuse.
    pub refuse_save: std::sync::atomic::AtomicBool,
    /// Makes `set_note` refuse.
    pub refuse_note: std::sync::atomic::AtomicBool,
    pub note_writes: Mutex<Vec<(Scope, String)>>,
    revision: AtomicU64,
    listeners: Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
}

impl Fake {
    pub fn new(modes: Vec<ClipMode>, rows: Vec<ClipRow>) -> Arc<Self> {
        Arc::new(Self {
            modes,
            calls: Mutex::default(),
            refuse_copy: std::sync::atomic::AtomicBool::new(false),
            intro: std::sync::atomic::AtomicBool::new(false),
            rows: Mutex::new(rows),
            notes: Mutex::default(),
            edits: Mutex::default(),
            refuse_save: std::sync::atomic::AtomicBool::new(false),
            refuse_note: std::sync::atomic::AtomicBool::new(false),
            note_writes: Mutex::default(),
            revision: AtomicU64::new(1),
            listeners: Mutex::new(Vec::new()),
        })
    }

    fn note_text(&self, scope: &Scope) -> Option<String> {
        let notes = self.notes.lock().expect("notes");
        notes
            .iter()
            .find_map(|(at, text)| (at == scope).then(|| text.clone()))
    }

    pub fn change(&self, rows: Vec<ClipRow>, revision: u64) {
        *self.rows.lock().expect("rows") = rows;
        self.revision.store(revision, Ordering::SeqCst);
        for listener in self.listeners.lock().expect("listeners").iter() {
            listener();
        }
    }
}

impl ClipboardHooks for Fake {
    fn counts(&self) -> Option<ClipCounts> {
        let history = self.rows.lock().expect("rows").len();
        Some(ClipCounts {
            history: u32::try_from(history).unwrap_or(u32::MAX),
            ..ClipCounts::default()
        })
    }

    fn modes(&self) -> Vec<ClipMode> {
        self.modes.clone()
    }

    fn search(&self, query: &str, mode: ClipMode, _: Option<&Location>) -> ClipResults {
        let rows = self.rows.lock().expect("rows");
        let wanted = |row: &&ClipRow| match mode {
            ClipMode::Recent => row.from == Origin::Clipboard,
            ClipMode::Saved => row.from == Origin::Saved && row.kind != ClipKind::Secret,
            ClipMode::Secrets => row.kind == ClipKind::Secret,
            ClipMode::Notes => row.kind == ClipKind::Note,
            ClipMode::All => true,
        };
        let rows: Vec<ClipRow> = rows
            .iter()
            .filter(wanted)
            .filter(|row| row.title.contains(query))
            .cloned()
            .collect();
        results(rows, self.revision.load(Ordering::SeqCst))
    }

    fn copy(&self, id: ClipId, how: How) -> Result<(), String> {
        self.calls.lock().expect("calls").push(Call::Copy(id, how));
        if self.refuse_copy.load(Ordering::SeqCst) {
            return Err("clipboard busy".to_owned());
        }
        Ok(())
    }
    fn delete(&self, id: ClipId) -> Result<(), String> {
        self.calls.lock().expect("calls").push(Call::Delete(id));
        let rows: Vec<ClipRow> = {
            let mut rows = self.rows.lock().expect("rows").clone();
            if let Some(gone) = rows
                .iter()
                .find(|row| row.id == id && row.kind == ClipKind::Note)
            {
                self.notes
                    .lock()
                    .expect("notes")
                    .retain(|(at, _)| *at != gone.scope);
            }
            rows.retain(|row| row.id != id);
            rows
        };
        self.change(rows, self.revision.load(Ordering::SeqCst) + 1);
        Ok(())
    }
    fn clear_history(&self) -> Result<(), String> {
        Ok(())
    }
    fn delete_all(&self) -> Result<(), String> {
        Ok(())
    }
    fn set_recording(&self, _: bool) -> Result<(), String> {
        Ok(())
    }
    fn subscribe(&self, on_change: Box<dyn Fn() + Send + Sync>) {
        self.listeners.lock().expect("listeners").push(on_change);
    }
    fn save_from_history(&self, id: ClipId) -> Result<ClipId, String> {
        self.calls
            .lock()
            .expect("calls")
            .push(Call::SaveFromHistory(id));
        let saved = ClipId(id.0 + 100);
        let mut rows = self.rows.lock().expect("rows").clone();
        rows.push(row(saved.0, ClipKind::Text, "saved copy", Origin::Saved));
        self.change(rows, self.revision.load(Ordering::SeqCst) + 1);
        Ok(saved)
    }
    fn save(&self, draft: SavedDraft) -> Result<ClipId, String> {
        self.calls
            .lock()
            .expect("calls")
            .push(Call::Save(Draft::of(&draft)));
        if self.refuse_save.load(Ordering::SeqCst) {
            return Err("store is busy".to_owned());
        }
        Ok(ClipId(900))
    }
    fn saved_for_edit(&self, id: ClipId) -> Result<SavedEdit, String> {
        let edits = self.edits.lock().expect("edits");
        let (_, edit) = edits.iter().find(|(at, _)| *at == id).ok_or("no edit")?;
        Ok(SavedEdit {
            kind: edit.kind,
            title: edit.title.clone(),
            body: SecretText::new(edit.body.clone()),
            user: edit.user.clone(),
            scope: edit.scope.clone(),
        })
    }
    fn update_saved(&self, id: ClipId, draft: SavedDraft) -> Result<(), String> {
        self.calls
            .lock()
            .expect("calls")
            .push(Call::Update(id, Draft::of(&draft)));
        if self.refuse_save.load(Ordering::SeqCst) {
            return Err("store is busy".to_owned());
        }
        Ok(())
    }
    fn rename_note(&self, from: Scope, to: Scope) -> Result<(), String> {
        self.calls
            .lock()
            .expect("calls")
            .push(Call::RenameNote(from, to));
        Ok(())
    }
    fn note_for(&self, folder: &Location) -> Option<String> {
        self.note_text(&Scope::Folder(folder.clone()))
    }
    fn global_note(&self) -> Option<String> {
        self.note_text(&Scope::Everywhere)
    }
    fn set_note(&self, scope: Scope, text: String) -> Result<(), String> {
        self.note_writes
            .lock()
            .expect("note writes")
            .push((scope.clone(), text.clone()));
        if self.refuse_note.load(Ordering::SeqCst) {
            return Err("The note could not be saved.".into());
        }
        let mut notes = self.notes.lock().expect("notes");
        notes.retain(|(at, _)| *at != scope);
        if !text.is_empty() {
            notes.push((scope, text));
        }
        Ok(())
    }
    fn reveal(&self, id: ClipId) -> Result<SecretText, String> {
        self.calls.lock().expect("calls").push(Call::Reveal(id));
        Ok(SecretText::new(SECRET.into()))
    }
    fn copy_user(&self, id: ClipId) -> Result<(), String> {
        self.calls.lock().expect("calls").push(Call::CopyUser(id));
        Ok(())
    }
    fn take_intro(&self) -> bool {
        self.intro.swap(false, Ordering::SeqCst)
    }
}

struct Harness {
    palette: Option<Entity<ClipPalette>>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(self.palette.clone())
    }
}

pub(super) struct Opened {
    window: AnyWindowHandle,
    harness: Entity<Harness>,
    pub palette: Entity<ClipPalette>,
    pub events: Rc<RefCell<Vec<ClipPaletteEvent>>>,
}

pub(super) fn open(cx: &mut TestAppContext, hooks: Arc<dyn ClipboardHooks>) -> Opened {
    open_in(cx, hooks, Place::Window)
}

pub(super) fn open_in(
    cx: &mut TestAppContext,
    hooks: Arc<dyn ClipboardHooks>,
    place: Place,
) -> Opened {
    open_at(cx, hooks, (place, None, None))
}

/// Opens the palette for a folder, with a probe that finds the folders that are gone.
pub(super) fn open_at(
    cx: &mut TestAppContext,
    hooks: Arc<dyn ClipboardHooks>,
    (place, folder, folder_probe): (Place, Option<Location>, Option<FolderProbe>),
) -> Opened {
    let events: Rc<RefCell<Vec<ClipPaletteEvent>>> = Rc::default();
    let sink = events.clone();
    let (window, harness) = open_sized(cx, px(1000.), move |window, cx| {
        let props = ClipPaletteProps {
            hooks,
            place,
            folder,
            key_cap: Some("ctrl-shift-v".into()),
            now_ms: Some(NOW),
            width: None,
            folder_probe,
        };
        let palette = cx.new(|cx| ClipPalette::new(props, window, cx));
        cx.subscribe(&palette, move |_, event: &ClipPaletteEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
        .detach();
        Harness {
            palette: Some(palette),
        }
    });
    cx.update(super::bind_keys);
    let palette = harness.read_with(cx, |harness, _| harness.palette.clone().expect("open"));
    let opened = Opened {
        window,
        harness,
        palette,
        events,
    };
    opened.settle(cx);
    opened
}

impl Opened {
    /// Closes the palette as the host does: nothing holds it any more.
    pub fn close(self, cx: &mut TestAppContext) {
        self.harness.update(cx, |harness, cx| {
            harness.palette = None;
            cx.notify();
        });
        self.settle(cx);
        drop(self.palette);
        cx.update(|_| ());
    }

    /// Waits `millis` on the test clock, then lets what was waiting run.
    pub fn wait(&self, millis: u64, cx: &mut TestAppContext) {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(millis));
        self.settle(cx);
    }

    pub fn settle(&self, cx: &mut TestAppContext) {
        cx.run_until_parked();
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
    }

    /// Hides the palette as the pop-up does, without closing it.
    pub fn reset(&self, cx: &mut TestAppContext) {
        let palette = self.palette.clone();
        cx.update_window(self.window, |_, window, cx| {
            palette.update(cx, |palette, cx| palette.reset(window, cx));
        })
        .expect("window");
    }

    /// Shows it again after a `reset`.
    pub fn wake(&self, cx: &mut TestAppContext) {
        self.palette.update(cx, |palette, cx| palette.wake(cx));
        self.settle(cx);
    }

    pub fn press(&self, key: &str, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.press(key, cx))
            .expect("window");
        self.settle(cx);
    }

    /// Reads the palette together with the window, as a focus check needs.
    pub fn window_read<R>(
        &self,
        cx: &mut TestAppContext,
        read: impl FnOnce(&ClipPalette, &Window, &gpui_kit::App) -> R,
    ) -> R {
        let palette = self.palette.clone();
        cx.update_window(self.window, |_, window, cx| {
            read(palette.read(cx), window, cx)
        })
        .expect("window")
    }

    pub fn type_text(&self, text: &str, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        self.settle(cx);
    }

    fn mode(&self, cx: &mut TestAppContext) -> ClipMode {
        self.palette.read_with(cx, |palette, _| palette.mode())
    }

    pub fn titles(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.palette.read_with(cx, |palette, _| {
            palette
                .state
                .results()
                .rows
                .iter()
                .map(|row| row.title.clone())
                .collect()
        })
    }

    pub fn query(&self, cx: &mut TestAppContext) -> String {
        self.palette
            .read_with(cx, |palette, cx| palette.query(cx).to_string())
    }

    pub fn label(&self, id: &'static str, cx: &mut TestAppContext) -> Option<String> {
        cx.update_window(self.window, |_, window, _| {
            window
                .within("clip-palette")
                .try_find(id)
                .and_then(|found| found.label().map(str::to_owned))
        })
        .expect("window")
    }
}

pub(super) fn history() -> Vec<ClipRow> {
    vec![
        row(1, ClipKind::Text, "alpha note", Origin::Clipboard),
        row(2, ClipKind::Path, "beta path", Origin::Clipboard),
        row(3, ClipKind::Text, "gamma snippet", Origin::Saved),
    ]
}

#[gpui_kit::test]
fn tab_cycles_only_the_offered_modes(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All, ClipMode::Recent], history());
    let palette = open(cx, fake);
    assert_eq!(palette.mode(cx), ClipMode::All);
    palette.press("tab", cx);
    assert_eq!(palette.mode(cx), ClipMode::Recent);
    palette.press("tab", cx);
    assert_eq!(palette.mode(cx), ClipMode::All);
    palette.press("shift-tab", cx);
    assert_eq!(palette.mode(cx), ClipMode::Recent);
}

#[gpui_kit::test]
fn tab_stays_put_with_one_offered_mode(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::Recent], history());
    let palette = open(cx, fake);
    assert_eq!(palette.mode(cx), ClipMode::Recent);
    palette.press("tab", cx);
    palette.press("shift-tab", cx);
    assert_eq!(palette.mode(cx), ClipMode::Recent);
    assert_eq!(palette.titles(cx), ["alpha note", "beta path"]);
}

#[gpui_kit::test]
fn an_empty_history_says_nothing_was_copied_yet(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All, ClipMode::Recent], Vec::new());
    let palette = open(cx, fake);
    let label = palette.label("clip-palette-empty", cx).unwrap_or_default();
    assert!(
        label.starts_with(&navo_l10n::tr("clip-empty-history-title")),
        "{label}"
    );
}

#[gpui_kit::test]
fn a_search_without_matches_says_so(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake);
    palette.type_text("zzz", cx);
    let label = palette.label("clip-palette-empty", cx).unwrap_or_default();
    assert!(
        label.starts_with(&navo_l10n::tr("clip-empty-search-title")),
        "{label}"
    );
}

#[gpui_kit::test]
fn a_new_snapshot_refreshes_and_an_older_revision_is_ignored(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake.clone());
    assert_eq!(palette.titles(cx).len(), 3);
    fake.change(
        vec![row(9, ClipKind::Text, "fresh copy", Origin::Clipboard)],
        7,
    );
    palette.settle(cx);
    assert_eq!(palette.titles(cx), ["fresh copy"]);
    fake.change(history(), 3);
    palette.settle(cx);
    assert_eq!(palette.titles(cx), ["fresh copy"]);
}

#[gpui_kit::test]
fn escape_clears_the_search_then_closes(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake);
    palette.type_text("alpha", cx);
    assert_eq!(palette.titles(cx), ["alpha note"]);
    palette.press("escape", cx);
    assert_eq!(palette.query(cx), "");
    assert_eq!(palette.titles(cx).len(), 3);
    assert!(palette.events.borrow().is_empty());
    palette.press("escape", cx);
    assert_eq!(*palette.events.borrow(), [ClipPaletteEvent::Dismissed]);
}

#[gpui_kit::test]
fn up_and_down_move_the_selection(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake);
    palette.press("down", cx);
    palette.press("down", cx);
    palette.press("down", cx);
    palette.press("up", cx);
    let selected = palette.palette.read_with(cx, |p, _| p.state.selected());
    assert_eq!(selected, 1);
}

#[test]
fn a_stale_request_or_older_revision_changes_nothing() {
    let mut state = PaletteState::new(vec![ClipMode::All]);
    let first = state.begin_request(false);
    assert!(state.accept(first, results(history(), 5)));
    let second = state.begin_request(true);
    assert!(!state.accept(first, results(Vec::new(), 6)));
    assert!(!state.accept(second, results(Vec::new(), 4)));
    assert_eq!(state.results().rows.len(), 3);
    assert!(state.accept(second, results(Vec::new(), 5)));
    assert!(state.results().rows.is_empty());
}

#[test]
fn a_refresh_keeps_the_selected_row_and_a_new_search_starts_at_the_top() {
    let mut state = PaletteState::new(vec![ClipMode::All]);
    let first = state.begin_request(false);
    state.accept(first, results(history(), 1));
    state.step(1);
    let mut moved = history();
    moved.insert(0, row(8, ClipKind::Text, "newest", Origin::Clipboard));
    let refresh = state.begin_request(true);
    state.accept(refresh, results(moved.clone(), 2));
    assert_eq!(state.selected_row().map(|row| row.id), Some(ClipId(2)));
    let search = state.begin_request(false);
    state.accept(search, results(moved, 3));
    assert_eq!(state.selected(), 0);
}

#[test]
fn modes_not_offered_are_refused_and_heads_group_only_in_all() {
    let mut state = PaletteState::new(vec![ClipMode::All, ClipMode::Saved]);
    assert!(!state.set_mode(ClipMode::Secrets));
    let request = state.begin_request(false);
    state.accept(request, results(history(), 1));
    assert_eq!(
        &**state.entries(),
        [
            Entry::Head(Origin::Clipboard),
            Entry::Row(0),
            Entry::Row(1),
            Entry::Head(Origin::Saved),
            Entry::Row(2),
        ]
    );
}

#[test]
fn a_secret_row_never_keeps_its_text() {
    let mut secret = row(4, ClipKind::Secret, "Build server", Origin::Saved);
    secret.detail = SECRET.into();
    let mut state = PaletteState::new(vec![ClipMode::All]);
    let request = state.begin_request(false);
    state.accept(request, results(vec![secret], 1));
    assert!(state.results().rows[0].detail.is_empty());
    assert!(state.has_user(ClipId(4)));
    assert!(!state.has_user(ClipId(5)));
}

#[test]
fn ages_read_as_short_words() {
    assert_eq!(age(NOW, NOW - 5_000), navo_l10n::tr("clip-age-now"));
    assert_eq!(age(NOW, NOW - 4 * MINUTE), "4 min");
    assert_eq!(age(NOW, NOW - 90 * MINUTE), "1 h");
    assert_eq!(
        age(NOW, NOW - 30 * 60 * MINUTE),
        navo_l10n::tr("clip-age-yesterday")
    );
    assert_eq!(age(NOW, NOW - 3 * 24 * 60 * MINUTE), "3 days");
}

#[test]
fn a_file_list_names_its_shared_folder_once() {
    let (folder, names) = file_list("D:\\build\\out\\a.elf\nD:\\build\\out\\b.map");
    assert_eq!(folder.as_deref(), Some("D:\\build\\out"));
    assert_eq!(names, ["a.elf", "b.map"]);
    let (folder, names) = file_list("D:\\one\\a.elf\nE:\\two\\b.map");
    assert_eq!(folder, None);
    assert_eq!(names.len(), 2);
}
