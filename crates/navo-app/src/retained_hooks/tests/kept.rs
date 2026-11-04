use super::*;
use navo_core::retention::{RetainedState as Stored, RetentionEntry};

/// A Ready entry kept by a failed delete whose original folder is gone.
fn kept(profile: &Profile) -> RetentionEntry {
    profile.fs.add_file("home/docs/a", 8);
    profile.failed_delete(&["home/docs/a"], |fs| fs.remove("home/docs"));
    let entry = stored(profile, profile.hooks().list(None, 10).unwrap()[0].id);
    assert!(entry.pinned && entry.kept && entry.state == Stored::Ready);
    entry
}

fn stored(profile: &Profile, id: RetentionId) -> RetentionEntry {
    let (db, _) = crate::crash::open_db(&profile.dirs.journal_file()).unwrap();
    navo_store::RetentionCatalog::new(db)
        .unwrap()
        .get(id)
        .unwrap()
}

/// Rewrites the row as an interrupted capture would leave it, below the catalog's checks.
fn rewrite(profile: &Profile, entry: &RetentionEntry, state: Stored, unrecorded: bool) {
    let mut record = serde_json::to_value(&entry.record).unwrap();
    if unrecorded {
        record["container_identity"] = serde_json::Value::Null;
    }
    let (db, _) = crate::crash::open_db(&profile.dirs.journal_file()).unwrap();
    db.connection()
        .execute(
            "UPDATE retained_entry SET state = ?2, record = ?3 WHERE id = ?1",
            rusqlite::params![
                entry.id.0,
                state.as_str(),
                serde_json::to_vec(&record).unwrap()
            ],
        )
        .unwrap();
}

#[test]
fn a_kept_preparing_entry_settles_before_the_user_deletes_it() {
    let profile = Profile::new();
    let entry = kept(&profile);
    rewrite(&profile, &entry, Stored::Preparing, false);
    profile.hooks().purge(entry.id).unwrap();
    assert!(profile.hooks().list(None, 10).unwrap().is_empty());
    assert!(profile.fs.metadata(&entry.record.held().unwrap()).is_err());
}

#[test]
fn a_kept_preparing_entry_whose_vault_is_gone_needs_attention_and_is_not_deleted() {
    let profile = Profile::new();
    let entry = kept(&profile);
    rewrite(&profile, &entry, Stored::Preparing, false);
    let vault = entry.record.anchor().unwrap();
    let moved = profile.at("home/moved-vault");
    profile
        .fs
        .rename_entry(&vault.path().to_text(), &moved.path().to_text());
    assert_eq!(
        profile.hooks().purge(entry.id),
        Err(navo_l10n::tr("retained-stuck"))
    );
    assert_eq!(stored(&profile, entry.id).state, Stored::Preparing);
    let name = entry.record.container.name().unwrap().to_text();
    assert!(profile.exists(&format!("home/moved-vault/{name}/payload")));
}

#[test]
fn a_conflict_without_a_recorded_holder_refuses_delete_with_its_own_reason() {
    for from in [Stored::Conflict, Stored::Preparing] {
        let profile = Profile::new();
        let entry = kept(&profile);
        rewrite(&profile, &entry, from, true);
        let hooks = profile.hooks();
        if from == Stored::Conflict {
            assert_eq!(hooks.list(None, 10).unwrap()[0].state, RetainedState::Stuck);
        }
        assert_eq!(hooks.purge(entry.id), Err(navo_l10n::tr("retained-stuck")));
        assert_eq!(
            stored(&profile, entry.id).state,
            Stored::Conflict,
            "{from:?}"
        );
        assert_eq!(hooks.list(None, 10).unwrap()[0].state, RetainedState::Stuck);
        assert!(profile.fs.metadata(&entry.record.held().unwrap()).is_ok());
    }
}
