use std::sync::Arc;
use std::sync::atomic::Ordering;

use gpui_kit::TestAppContext;
use navo_core::Location;

use super::edit_form::{Field, FormKind, Problem};
use super::manage_kinds::Kind;
use super::tests::{Call, Draft, Fake, Opened, open_at, row};
use super::{ClipId, ClipKind, ClipMode, ClipRow, FolderProbe, Origin, Place, Scope};

const HERE: &str = "/work/alpha";
const GONE: &str = "/work/gone";
const PASSWORD: &str = "hunter2-do-not-keep";
const MODES: [ClipMode; 5] = [
    ClipMode::All,
    ClipMode::Saved,
    ClipMode::Secrets,
    ClipMode::Notes,
    ClipMode::Recent,
];

fn folder(path: &str) -> Location {
    Location::new(path).expect("location")
}

fn library() -> Vec<ClipRow> {
    vec![
        row(3, ClipKind::Text, "gamma snippet", Origin::Saved),
        row(4, ClipKind::Path, "delta folder", Origin::Saved),
        row(5, ClipKind::Secret, "epsilon login", Origin::Saved),
    ]
}

fn stored(kind: ClipKind, title: &str, body: &str) -> Draft {
    Draft {
        kind,
        title: title.into(),
        body: body.into(),
        user: (kind == ClipKind::Secret).then(|| "admin".into()),
        scope: Scope::Folder(folder(HERE)),
    }
}

fn opened(cx: &mut TestAppContext, rows: Vec<ClipRow>) -> (Opened, Arc<Fake>) {
    let fake = Fake::new(MODES.to_vec(), rows);
    {
        let mut edits = fake.edits.lock().expect("edits");
        edits.push((
            ClipId(3),
            stored(ClipKind::Text, "gamma snippet", "echo gamma"),
        ));
        edits.push((
            ClipId(4),
            stored(ClipKind::Path, "delta folder", "/work/delta"),
        ));
        edits.push((ClipId(5), stored(ClipKind::Secret, "epsilon login", "")));
    }
    let palette = open_at(cx, fake.clone(), (Place::Window, Some(folder(HERE)), None));
    (palette, fake)
}

fn calls(fake: &Fake) -> Vec<Call> {
    std::mem::take(&mut *fake.calls.lock().expect("calls"))
}

fn is_open(palette: &Opened, cx: &mut TestAppContext) -> bool {
    palette.palette.read_with(cx, |p, _| p.form.is_some())
}

fn kind(palette: &Opened, cx: &mut TestAppContext) -> Option<FormKind> {
    palette
        .palette
        .read_with(cx, |p, _| p.form.as_ref().map(|f| f.kind))
}

fn asks(palette: &Opened, cx: &mut TestAppContext) -> bool {
    palette
        .palette
        .read_with(cx, |p, _| p.form.as_ref().is_some_and(|f| f.is_asking))
}

fn problem(palette: &Opened, cx: &mut TestAppContext) -> Option<String> {
    palette.palette.read_with(cx, |p, _| {
        let form = p.form.as_ref()?;
        form.problem.as_ref().map(Problem::text)
    })
}

fn values(palette: &Opened, cx: &mut TestAppContext) -> (String, String) {
    palette.palette.read_with(cx, |p, cx| {
        let form = p.form.as_ref().expect("the form is open");
        (
            form.title.read(cx).value().to_string(),
            form.body.read(cx).value().to_string(),
        )
    })
}

fn focused(palette: &Opened, cx: &mut TestAppContext) -> Option<Field> {
    palette.window_read(cx, |p, window, cx| {
        p.form.as_ref().and_then(|form| form.focused(window, cx))
    })
}

fn pick(palette: &Opened, mode: ClipMode, cx: &mut TestAppContext) {
    palette.palette.update(cx, |p, cx| p.pick_mode(mode, cx));
    palette.settle(cx);
}

#[gpui_kit::test]
fn ctrl_n_opens_a_new_form_from_the_saved_empty_state(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx, Vec::new());
    pick(&palette, ClipMode::Saved, cx);
    assert!(!is_open(&palette, cx));
    palette.press("ctrl-n", cx);
    assert_eq!(kind(&palette, cx), Some(FormKind::Snippet));
    assert_eq!(focused(&palette, cx), Some(Field::Title));
}

#[gpui_kit::test]
fn ctrl_n_in_secrets_starts_a_secret(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx, Vec::new());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("ctrl-n", cx);
    assert_eq!(kind(&palette, cx), Some(FormKind::Secret));
}

#[gpui_kit::test]
fn a_blank_title_refuses_and_says_so(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    palette.press("ctrl-n", cx);
    palette.press("ctrl-enter", cx);
    assert_eq!(calls(&fake), []);
    assert_eq!(problem(&palette, cx), Some(Problem::NoTitle.text()));
    palette.type_text("   ", cx);
    palette.press("ctrl-s", cx);
    assert_eq!(calls(&fake), []);
    assert!(is_open(&palette, cx));
    assert_eq!(focused(&palette, cx), Some(Field::Title));
}

#[gpui_kit::test]
fn a_title_over_eighty_characters_refuses(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    palette.press("ctrl-n", cx);
    palette.type_text(&"a".repeat(81), cx);
    palette.press("ctrl-enter", cx);
    assert_eq!(problem(&palette, cx), Some(Problem::TitleTooLong.text()));
    palette.press("backspace", cx);
    palette.press("ctrl-enter", cx);
    assert_eq!(calls(&fake).len(), 1);
}

#[gpui_kit::test]
fn a_new_snippet_saves_with_a_trimmed_title_and_its_text(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    palette.press("ctrl-n", cx);
    palette.type_text("  Greeting  ", cx);
    palette.press("tab", cx);
    assert_eq!(focused(&palette, cx), Some(Field::Body));
    palette.type_text("Hello", cx);
    palette.press("enter", cx);
    palette.type_text("there", cx);
    palette.press("ctrl-enter", cx);
    let draft = Draft {
        kind: ClipKind::Text,
        title: "Greeting".into(),
        body: "Hello\nthere".into(),
        user: None,
        scope: Scope::Everywhere,
    };
    assert_eq!(calls(&fake), [Call::Save(draft)]);
    assert!(!is_open(&palette, cx));
}

#[gpui_kit::test]
fn a_secret_saves_through_save_and_the_form_never_shows_it_after(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("ctrl-n", cx);
    palette.type_text("Build server", cx);
    palette.press("tab", cx);
    palette.type_text("admin", cx);
    palette.press("tab", cx);
    assert_eq!(focused(&palette, cx), Some(Field::Password));
    palette.type_text(PASSWORD, cx);
    palette.press("alt-r", cx);
    let shown = palette.palette.read_with(cx, |p, _| {
        let form = p.form.as_ref()?;
        Some((form.password.is_shown(), form.password.text().to_owned()))
    });
    assert_eq!(shown, Some((true, PASSWORD.to_owned())));
    palette.press("ctrl-enter", cx);
    let draft = Draft {
        kind: ClipKind::Secret,
        title: "Build server".into(),
        body: PASSWORD.into(),
        user: Some("admin".into()),
        scope: Scope::Everywhere,
    };
    assert_eq!(calls(&fake), [Call::Save(draft)]);
    assert!(!is_open(&palette, cx));
    assert!(
        palette
            .palette
            .read_with(cx, |p, _| p.overlay.revealed.is_none())
    );
}

#[gpui_kit::test]
fn a_new_secret_needs_a_password(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("ctrl-n", cx);
    palette.type_text("Build server", cx);
    palette.press("ctrl-enter", cx);
    assert_eq!(calls(&fake), []);
    assert_eq!(problem(&palette, cx), Some(Problem::NoPassword.text()));
    assert_eq!(focused(&palette, cx), Some(Field::Password));
}

#[gpui_kit::test]
fn editing_a_secret_with_an_empty_password_keeps_it(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("f2", cx);
    assert_eq!(
        values(&palette, cx),
        ("epsilon login".into(), String::new())
    );
    palette.type_text(" 2", cx);
    palette.press("ctrl-enter", cx);
    let draft = Draft {
        kind: ClipKind::Secret,
        title: "epsilon login 2".into(),
        body: String::new(),
        user: Some("admin".into()),
        scope: Scope::Folder(folder(HERE)),
    };
    assert_eq!(calls(&fake), [Call::Update(ClipId(5), draft)]);
}

#[gpui_kit::test]
fn a_secret_changed_into_another_kind_needs_its_new_text(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("f2", cx);
    palette.press("shift-tab", cx);
    assert_eq!(focused(&palette, cx), Some(Field::Kind));
    palette.press("left", cx);
    assert_eq!(kind(&palette, cx), Some(FormKind::Path));
    palette.press("ctrl-enter", cx);
    assert_eq!(problem(&palette, cx), Some(Problem::NoBody.text()));
    assert_eq!(calls(&fake), []);
}

#[gpui_kit::test]
fn escape_on_a_changed_form_asks_and_enter_discards(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    palette.press("ctrl-n", cx);
    palette.type_text("x", cx);
    palette.press("escape", cx);
    assert!(is_open(&palette, cx) && asks(&palette, cx));
    palette.press("escape", cx);
    assert!(is_open(&palette, cx) && !asks(&palette, cx));
    palette.press("escape", cx);
    palette.press("enter", cx);
    assert!(!is_open(&palette, cx));
    assert_eq!(calls(&fake), []);
}

#[gpui_kit::test]
fn escape_on_an_untouched_form_closes_it_at_once(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx, library());
    palette.press("ctrl-n", cx);
    palette.press("escape", cx);
    assert!(!is_open(&palette, cx));
    assert!(palette.events.borrow().is_empty());
}

#[gpui_kit::test]
fn a_typed_password_counts_as_a_change_and_discarding_drops_it(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx, library());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("f2", cx);
    palette.press("tab", cx);
    palette.press("tab", cx);
    palette.type_text("abc", cx);
    palette.press("backspace", cx);
    palette.press("escape", cx);
    assert!(asks(&palette, cx));
    palette.press("enter", cx);
    assert!(!is_open(&palette, cx));
}

#[gpui_kit::test]
fn f2_on_a_saved_row_loads_its_values_and_scope(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    pick(&palette, ClipMode::Saved, cx);
    palette.press("f2", cx);
    let loaded = ("gamma snippet".to_owned(), "echo gamma".to_owned());
    assert_eq!(values(&palette, cx), loaded);
    let scope = palette.palette.read_with(cx, |p, _| {
        let form = p.form.as_ref()?;
        Some(form.scopes[form.scope].clone())
    });
    assert_eq!(scope, Some(Scope::Folder(folder(HERE))));
    palette.type_text("!", cx);
    palette.press("ctrl-enter", cx);
    let saved = calls(&fake);
    let [Call::Update(ClipId(3), draft)] = saved.as_slice() else {
        panic!("{saved:?}");
    };
    assert_eq!(draft.title, "gamma snippet!");
    assert_eq!(draft.body, "echo gamma");
}

#[gpui_kit::test]
fn a_refused_save_keeps_the_form_and_the_password(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    fake.refuse_save.store(true, Ordering::SeqCst);
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("ctrl-n", cx);
    palette.type_text("Build server", cx);
    palette.press("tab", cx);
    palette.press("tab", cx);
    palette.type_text(PASSWORD, cx);
    palette.press("ctrl-s", cx);
    assert_eq!(problem(&palette, cx).as_deref(), Some("store is busy"));
    fake.refuse_save.store(false, Ordering::SeqCst);
    palette.press("ctrl-s", cx);
    let saved = calls(&fake);
    assert_eq!(saved.len(), 2);
    assert!(matches!(&saved[1], Call::Save(draft) if draft.body == PASSWORD));
}

#[gpui_kit::test]
fn the_library_opens_the_form_for_its_selected_row_and_returns_to_it(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx, library());
    palette.palette.update(cx, |p, cx| {
        p.open_manage(cx);
        p.pick_kind(Kind::Snippets, cx);
    });
    palette.settle(cx);
    palette.press("f2", cx);
    assert_eq!(values(&palette, cx).0, "gamma snippet");
    palette.press("escape", cx);
    assert!(!is_open(&palette, cx));
    assert!(palette.palette.read_with(cx, |p, _| p.manage.is_some()));
}

#[gpui_kit::test]
fn the_form_swallows_palette_keys_meant_for_the_list(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx, library());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("ctrl-n", cx);
    for key in ["alt-r", "alt-u", "ctrl-k", "delete", "ctrl-e"] {
        palette.press(key, cx);
    }
    assert_eq!(calls(&fake), []);
    assert!(
        palette
            .palette
            .read_with(cx, |p, _| p.overlay.menu.is_none())
    );
}

fn gone_note() -> ClipRow {
    let mut note = row(210, ClipKind::Note, "gone", Origin::Saved);
    note.scope = Scope::Folder(folder(GONE));
    note
}

fn open_notes(cx: &mut TestAppContext) -> (Opened, Arc<Fake>) {
    let fake = Fake::new(vec![ClipMode::Notes], vec![gone_note()]);
    let probe: FolderProbe = Arc::new(|at: &Location| at.as_str() == GONE);
    let args = (Place::Window, Some(folder(HERE)), Some(probe));
    (open_at(cx, fake.clone(), args), fake)
}

fn select_gone(palette: &Opened, cx: &mut TestAppContext) {
    let at = palette.titles(cx).iter().position(|title| title == "gone");
    let at = at.expect("the gone note is listed");
    palette.palette.update(cx, |p, cx| p.select(at, cx));
}

#[gpui_kit::test]
fn f2_on_a_note_whose_folder_is_gone_offers_to_point_it_here(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx);
    select_gone(&palette, cx);
    palette.press("f2", cx);
    assert!(palette.palette.read_with(cx, |p, _| p.repoint.is_some()));
    palette.press("escape", cx);
    assert!(palette.palette.read_with(cx, |p, _| p.repoint.is_none()));
    assert_eq!(calls(&fake), []);
    palette.press("f2", cx);
    palette.press("enter", cx);
    let to = Scope::Folder(folder(HERE));
    assert_eq!(
        calls(&fake),
        [Call::RenameNote(Scope::Folder(folder(GONE)), to)]
    );
}

#[gpui_kit::test]
fn pointing_a_note_at_a_folder_that_has_one_says_so_and_moves_nothing(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx);
    fake.notes
        .lock()
        .expect("notes")
        .push((Scope::Folder(folder(HERE)), "mine".into()));
    select_gone(&palette, cx);
    palette.press("f2", cx);
    assert!(palette.palette.read_with(cx, |p, _| p.repoint.is_none()));
    let said = palette.label("clip-palette-status", cx);
    assert_eq!(said.as_deref(), Some("That folder already has a note"));
    assert_eq!(calls(&fake), []);
}

#[gpui_kit::test]
fn pasting_a_password_drops_line_breaks_and_keeps_the_rest(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx, library());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("ctrl-n", cx);
    palette.press("tab", cx);
    palette.press("tab", cx);
    assert_eq!(focused(&palette, cx), Some(Field::Password));
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
        "pa\r\nss\tw0rd\n".into(),
    ));
    palette.press("ctrl-v", cx);
    let held = palette.palette.read_with(cx, |p, _| {
        p.form.as_ref().map(|f| f.password.text().to_owned())
    });
    assert_eq!(held.as_deref(), Some("passw0rd"));
}
