use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use chrono::Utc;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use rusty_core::AppState;
use rusty_core::Location;
use rusty_core::retention::RetentionId;
use rusty_fs::FakeFs;
use rusty_l10n::tr;

use super::undo::{Rig, files};
use super::{open_options, options_for};
use crate::browser::{RetainedHooks, RetainedKind, RetainedRow, RetainedState};
use crate::recently_deleted::{MAX_ROWS, PAGE_ROWS};
use crate::window_view::keys::OpenRecentlyDeleted;
use crate::window_view::recent_sheet::load_all;

const HOUR: i64 = 3_600_000;

#[derive(Default)]
struct Shelf {
    rows: Mutex<Vec<RetainedRow>>,
    restored: Mutex<Vec<RetentionId>>,
    purged: Mutex<Vec<RetentionId>>,
    pages: Mutex<Vec<(Option<RetentionId>, u16)>>,
    refusal: Mutex<Option<String>>,
}

fn locked<T>(slot: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

impl RetainedHooks for Shelf {
    fn list(&self, after: Option<RetentionId>, limit: u16) -> Result<Vec<RetainedRow>, String> {
        locked(&self.pages).push((after, limit));
        let rows = locked(&self.rows);
        Ok(rows
            .iter()
            .filter(|row| after.is_none_or(|after| row.id > after))
            .take(usize::from(limit))
            .cloned()
            .collect())
    }

    fn restore(&self, id: RetentionId) -> Result<Location, String> {
        if let Some(reason) = locked(&self.refusal).clone() {
            return Err(reason);
        }
        locked(&self.restored).push(id);
        let rows = locked(&self.rows);
        let row = rows.iter().find(|row| row.id == id);
        row.map(|row| row.original.clone())
            .ok_or_else(|| tr("retained-missing"))
    }

    fn purge(&self, id: RetentionId) -> Result<(), String> {
        locked(&self.purged).push(id);
        Ok(())
    }
}

fn row(fs: &FakeFs, (id, name): (i64, &str), ago_ms: i64) -> RetainedRow {
    let deleted_at_ms = Utc::now().timestamp_millis() - ago_ms;
    RetainedRow {
        id: RetentionId(id),
        original: fs.location(&format!("home/{name}")),
        kind: RetainedKind::Delete,
        bytes: 2048,
        deleted_at_ms,
        expires_at_ms: deleted_at_ms + 72 * HOUR,
        state: RetainedState::Ready,
    }
}

fn three(fs: &FakeFs) -> Vec<RetainedRow> {
    vec![
        row(fs, (1, "old.txt"), 50 * HOUR),
        row(fs, (2, "mid.txt"), 26 * HOUR),
        row(fs, (3, "new.txt"), HOUR),
    ]
}

fn open(cx: &mut TestAppContext, fs: Arc<FakeFs>, shelf: &Arc<Shelf>) -> Rig {
    let mut options = options_for(fs.clone(), AppState::new(fs.location("home")));
    options.retained = shelf.clone();
    let (window, view) = open_options(cx, options);
    Rig { window, view }
}

fn run_command(cx: &mut TestAppContext, rig: &Rig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(OpenRecentlyDeleted), cx);
    })
    .expect("window");
    rig.wait(cx, Duration::from_millis(200));
}

fn opened(
    cx: &mut TestAppContext,
    make: impl FnOnce(&FakeFs) -> Vec<RetainedRow>,
) -> (Rig, Arc<Shelf>) {
    let fs = files();
    let shelf = Arc::new(Shelf {
        rows: Mutex::new(make(&fs)),
        ..Shelf::default()
    });
    let rig = open(cx, fs, &shelf);
    run_command(cx, &rig);
    (rig, shelf)
}

fn click(cx: &mut TestAppContext, rig: &Rig, id: &'static str) {
    cx.update_window(rig.window, |_, window, cx| window.click(id, cx))
        .expect("window");
    rig.wait(cx, Duration::from_millis(200));
}

#[gpui_kit::test]
fn the_command_opens_the_sheet_with_the_newest_row_first(cx: &mut TestAppContext) {
    let (rig, _) = opened(cx, three);
    let shown = rig.view.read_with(cx, |view, _| view.recent_view());
    assert!(shown.is_open);
    assert_eq!(shown.items, [3, 2, 1]);
    assert_eq!(shown.cursor, Some(3));
    let has_sheet = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("recently-deleted-sheet").is_some()
        })
        .expect("window");
    assert!(has_sheet);
}

#[gpui_kit::test]
fn enter_restores_the_selected_row(cx: &mut TestAppContext) {
    let (rig, shelf) = opened(cx, three);
    rig.press(cx, &["down", "enter"]);
    assert_eq!(*locked(&shelf.restored), [RetentionId(2)]);
    let shown = rig.view.read_with(cx, |view, _| view.recent_view());
    assert_eq!(shown.items, [3, 1]);
    assert_eq!(shown.working, 0);
    let status = shown.status.unwrap_or_default();
    assert!(status.starts_with("Restored mid.txt"), "{status}");
}

#[gpui_kit::test]
fn a_refused_restore_keeps_the_row_and_says_why(cx: &mut TestAppContext) {
    let (rig, shelf) = opened(cx, three);
    let reason = tr("retained-busy");
    *locked(&shelf.refusal) = Some(reason.clone());
    rig.press(cx, &["enter"]);
    let shown = rig.view.read_with(cx, |view, _| view.recent_view());
    assert_eq!(shown.items, [3, 2, 1]);
    assert_eq!(shown.status, Some(reason));
}

#[gpui_kit::test]
fn delete_asks_first_and_only_the_confirmation_purges(cx: &mut TestAppContext) {
    let (rig, shelf) = opened(cx, three);
    rig.press(cx, &["delete"]);
    assert!(
        rig.view
            .read_with(cx, |view, _| view.recent_view().is_asking)
    );
    assert!(locked(&shelf.purged).is_empty());
    rig.press(cx, &["escape"]);
    assert!(
        !rig.view
            .read_with(cx, |view, _| view.recent_view().is_asking)
    );
    assert!(locked(&shelf.purged).is_empty());
    rig.press(cx, &["delete"]);
    click(cx, &rig, "confirm-button");
    assert_eq!(*locked(&shelf.purged), [RetentionId(3)]);
    let shown = rig.view.read_with(cx, |view, _| view.recent_view());
    assert_eq!(shown.items, [2, 1]);
    assert_eq!(shown.cursor, Some(2));
    assert!(!shown.is_asking);
}

fn needy(fs: &FakeFs) -> Vec<RetainedRow> {
    let mut rows = three(fs);
    rows[2].state = RetainedState::Busy;
    rows[1].state = RetainedState::NeedsAttention;
    rows
}

#[gpui_kit::test]
fn busy_and_attention_rows_refuse_with_a_reason(cx: &mut TestAppContext) {
    let (rig, shelf) = opened(cx, needy);
    let status =
        |cx: &mut TestAppContext| rig.view.read_with(cx, |view, _| view.recent_view().status);
    rig.press(cx, &["enter"]);
    assert_eq!(status(cx), Some(tr("retained-busy")));
    rig.press(cx, &["down", "delete"]);
    assert_eq!(status(cx), Some(tr("retained-not-ready")));
    assert!(
        !rig.view
            .read_with(cx, |view, _| view.recent_view().is_asking)
    );
    assert!(locked(&shelf.restored).is_empty());
    assert!(locked(&shelf.purged).is_empty());
}

#[gpui_kit::test]
fn an_empty_shelf_shows_the_empty_text(cx: &mut TestAppContext) {
    let (rig, _) = opened(cx, |_| Vec::new());
    let label = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .find("recently-deleted-message")
                .label()
                .map(str::to_owned)
        })
        .expect("window");
    assert_eq!(label, Some(tr("recently-deleted-empty-title")));
}

#[gpui_kit::test]
fn escape_closes_the_sheet(cx: &mut TestAppContext) {
    let (rig, _) = opened(cx, |_| Vec::new());
    rig.press(cx, &["escape"]);
    assert!(!rig.view.read_with(cx, |view, _| view.recent_view().is_open));
}

fn numbered(fs: &FakeFs, count: i64) -> Vec<RetainedRow> {
    (1..=count).map(|id| row(fs, (id, "n.txt"), HOUR)).collect()
}

#[test]
fn rows_load_in_pages_of_200_until_a_short_page() {
    let fs = FakeFs::new();
    let shelf = Shelf {
        rows: Mutex::new(numbered(&fs, 450)),
        ..Shelf::default()
    };
    let rows = load_all(&shelf).expect("rows");
    assert_eq!(rows.len(), 450);
    assert_eq!(
        *locked(&shelf.pages),
        [
            (None, PAGE_ROWS),
            (Some(RetentionId(200)), PAGE_ROWS),
            (Some(RetentionId(400)), PAGE_ROWS)
        ]
    );
}

#[test]
fn past_the_row_cap_the_newest_rows_are_kept() {
    let fs = FakeFs::new();
    let extra = i64::try_from(MAX_ROWS).expect("cap") + 300;
    let shelf = Shelf {
        rows: Mutex::new(numbered(&fs, extra)),
        ..Shelf::default()
    };
    let rows = load_all(&shelf).expect("rows");
    assert_eq!(rows.len(), MAX_ROWS);
    assert_eq!(rows.first().map(|row| row.id), Some(RetentionId(301)));
    assert_eq!(rows.last().map(|row| row.id), Some(RetentionId(extra)));
}
