use std::sync::Arc;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use rusty_core::retention::RetentionId;
use rusty_core::{AppState, Location};
use rusty_fs::FakeFs;
use rusty_l10n::tr;

use super::options_for;
use super::undo::{Journal, ORIGINAL, Rig, State, files, names};
use crate::browser::{RenameHooks, RetainedHooks, RetainedRow};

struct Vault {
    journal: Journal,
    fs: Arc<FakeFs>,
    busy: Option<RetentionId>,
}

impl RetainedHooks for Vault {
    fn list(&self, _: Option<RetentionId>, _: u16) -> Result<Vec<RetainedRow>, String> {
        Ok(Vec::new())
    }

    fn restore(&self, id: RetentionId) -> Result<Location, String> {
        if self.busy == Some(id) {
            return Err(tr("retained-busy"));
        }
        let mut kept = self.journal.kept();
        let at = usize::try_from(id.0 - 1).map_err(|e| e.to_string())?;
        let entry = kept
            .get_mut(at)
            .filter(|entry| entry.is_live)
            .ok_or_else(|| tr("retained-missing"))?;
        entry.is_live = false;
        let name = entry.location.name().map(|name| name.to_text());
        self.fs
            .add_file(&format!("home/{}", name.unwrap_or_default()), 1);
        Ok(entry.location.clone())
    }

    fn purge(&self, id: RetentionId) -> Result<(), String> {
        let at = usize::try_from(id.0 - 1).map_err(|e| e.to_string())?;
        self.journal.kept()[at].is_live = false;
        Ok(())
    }
}

fn open(cx: &mut TestAppContext, journal: &Journal, busy: Option<RetentionId>) -> Rig {
    open_hooks(cx, journal, busy, journal.hooks())
}

fn open_hooks(
    cx: &mut TestAppContext,
    journal: &Journal,
    busy: Option<RetentionId>,
    hooks: RenameHooks,
) -> Rig {
    let fs = files();
    let mut options = options_for(fs.clone(), AppState::new(fs.location("home")));
    options.operations = journal.operations();
    options.rename = Some(hooks);
    options.retained = Arc::new(Vault {
        journal: journal.clone(),
        fs,
        busy,
    });
    let (window, view) = super::open_options(cx, options);
    Rig { window, view }
}

impl Rig {
    fn confirm_focused(&self, cx: &mut TestAppContext) -> bool {
        self.window
            .update(cx, |_, window, _| {
                window.try_find("confirm").and_then(|found| found.focused()) == Some(true)
            })
            .expect("window")
    }

    fn shift_delete(&self, cx: &mut TestAppContext, keys: &[&str]) {
        self.press(cx, keys);
        self.press(cx, &["shift-delete"]);
        if !self.confirm_focused(cx) {
            self.press(cx, &["right"]);
        }
        self.press(cx, &["enter"]);
    }
}

const FIRST_TWO: [&str; 2] = ["down", "shift-down"];
const FIRST_THREE: [&str; 3] = ["down", "shift-down", "shift-down"];

#[gpui_kit::test]
fn shift_delete_of_two_files_then_ctrl_z_restores_both(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let rig = open(cx, &journal, None);
    rig.shift_delete(cx, &FIRST_TWO);
    assert_eq!(rig.listing(cx), names(&["docs", "c.txt", "d.txt"]));
    rig.press(cx, &["ctrl-z"]);
    assert_eq!(rig.listing(cx), names(&ORIGINAL));
    assert_eq!(rig.notice(cx).as_deref(), Some("Restored 2 items"));
    assert_eq!(journal.states(), [State::Undone]);
    rig.press(cx, &["ctrl-z"]);
    assert_eq!(rig.notice(cx).as_deref(), Some("Nothing to undo"));
}

#[gpui_kit::test]
fn an_entry_purged_in_between_is_reported_and_the_rest_restored(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let (purger, retire) = (journal.clone(), journal.hooks().retire);
    let hooks = RenameHooks {
        retire: Arc::new(move |offer| {
            for kept in purger.kept().iter_mut() {
                kept.is_live &= kept.location.name().is_none_or(|n| n.to_text() != "b.txt");
            }
            retire(offer)
        }),
        ..journal.hooks()
    };
    let rig = open_hooks(cx, &journal, None, hooks);
    rig.shift_delete(cx, &FIRST_THREE);
    rig.press(cx, &["ctrl-z"]);
    assert_eq!(rig.listing(cx), names(&["docs", "a.txt", "c.txt", "d.txt"]));
    assert_eq!(
        rig.notice(cx).as_deref(),
        Some(
            "Restored 2 of 3 items. 1 could not be restored: \
             This item is no longer in Recently deleted."
        )
    );
}

#[gpui_kit::test]
fn a_partial_restore_retires_the_job_and_leaves_the_rest_kept(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let rig = open(cx, &journal, Some(RetentionId(2)));
    rig.shift_delete(cx, &FIRST_TWO);
    rig.press(cx, &["ctrl-z"]);
    assert!(
        rig.notice(cx)
            .is_some_and(|text| text.starts_with("Restored 1 of 2 items. 1 could not"))
    );
    assert!(journal.kept()[1].is_live);
    assert_eq!(journal.states(), [State::Undone]);
    rig.press(cx, &["ctrl-z"]);
    assert_eq!(rig.notice(cx).as_deref(), Some("Nothing to undo"));
}

#[gpui_kit::test]
fn ctrl_z_undoes_the_newest_job_of_either_kind_first(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let rig = open(cx, &journal, None);
    rig.rename_first(cx);
    rig.shift_delete(cx, &["home", "down", "shift-down"]);
    assert_eq!(rig.listing(cx), names(&["docs", "d.txt", "renamed.txt"]));
    rig.undo(cx);
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "b.txt", "c.txt", "d.txt", "renamed.txt"])
    );
    rig.undo(cx);
    assert_eq!(rig.listing(cx), names(&ORIGINAL));
    assert_eq!(journal.states(), [State::Undone, State::Undone]);
}

#[gpui_kit::test]
fn a_rename_after_a_delete_is_undone_before_the_delete(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let rig = open(cx, &journal, None);
    rig.shift_delete(cx, &["down", "down", "down", "down"]);
    rig.rename_first(cx);
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "b.txt", "c.txt", "renamed.txt"])
    );
    rig.undo(cx);
    assert_eq!(rig.listing(cx), names(&["docs", "a.txt", "b.txt", "c.txt"]));
    rig.undo(cx);
    assert_eq!(rig.listing(cx), names(&ORIGINAL));
}

#[gpui_kit::test]
fn a_journal_error_before_the_restore_keeps_everything_deleted(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let hooks = RenameHooks {
        retire: Arc::new(|_| Err("disk gone".to_owned())),
        ..journal.hooks()
    };
    let rig = open_hooks(cx, &journal, None, hooks);
    rig.shift_delete(cx, &FIRST_TWO);
    rig.press(cx, &["ctrl-z"]);
    assert_eq!(rig.listing(cx), names(&["docs", "c.txt", "d.txt"]));
    assert_eq!(
        rig.notice(cx).as_deref(),
        Some("The undo could not start: disk gone")
    );
    assert!(journal.kept().iter().all(|kept| kept.is_live));
}
