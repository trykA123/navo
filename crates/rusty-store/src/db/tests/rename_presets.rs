use super::*;

const RULES: &str = r#"{"v":1,"mask":"[N]"}"#;

fn names(db: &Db) -> Vec<String> {
    db.presets()
        .unwrap()
        .named
        .into_iter()
        .map(|preset| preset.name)
        .collect()
}

#[test]
fn a_fresh_database_has_no_presets() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(db.presets().unwrap(), RenamePresets::default());
}

#[test]
fn a_saved_preset_reads_back() {
    let db = Db::open_in_memory().unwrap();
    db.save_preset("numbered", RULES, 100).unwrap();
    assert_eq!(
        db.presets().unwrap().named,
        [RenamePreset {
            name: "numbered".to_owned(),
            rules: RULES.to_owned(),
            saved_at: 100,
        }]
    );
}

#[test]
fn presets_come_back_sorted_by_name() {
    let db = Db::open_in_memory().unwrap();
    for name in ["photos", "archive", "notes"] {
        db.save_preset(name, RULES, 1).unwrap();
    }
    assert_eq!(names(&db), ["archive", "notes", "photos"]);
}

#[test]
fn saving_the_same_name_replaces_rules_and_time() {
    let db = Db::open_in_memory().unwrap();
    db.save_preset("numbered", RULES, 100).unwrap();
    db.save_preset("numbered", r#"{"v":1,"mask":"[N]_[C]"}"#, 200)
        .unwrap();
    let named = db.presets().unwrap().named;
    assert_eq!(named.len(), 1);
    assert_eq!(named[0].rules, r#"{"v":1,"mask":"[N]_[C]"}"#);
    assert_eq!(named[0].saved_at, 200);
}

#[test]
fn last_used_is_returned_apart_from_the_named_presets() {
    let db = Db::open_in_memory().unwrap();
    db.save_preset("", RULES, 50).unwrap();
    db.save_preset("numbered", RULES, 100).unwrap();
    let presets = db.presets().unwrap();
    assert_eq!(names(&db), ["numbered"]);
    assert_eq!(presets.last_used.map(|preset| preset.saved_at), Some(50));
}

#[test]
fn last_used_is_replaced_by_the_next_run() {
    let db = Db::open_in_memory().unwrap();
    db.save_preset("", RULES, 50).unwrap();
    db.save_preset("", "{}", 60).unwrap();
    let last = db.presets().unwrap().last_used.unwrap();
    assert_eq!((last.rules.as_str(), last.saved_at), ("{}", 60));
}

#[test]
fn deleting_removes_only_that_preset() {
    let db = Db::open_in_memory().unwrap();
    db.save_preset("", RULES, 1).unwrap();
    db.save_preset("a", RULES, 1).unwrap();
    db.save_preset("b", RULES, 1).unwrap();
    db.delete_preset("a").unwrap();
    let presets = db.presets().unwrap();
    assert_eq!(names(&db), ["b"]);
    assert!(presets.last_used.is_some());
}

#[test]
fn deleting_an_unknown_name_is_not_an_error() {
    let db = Db::open_in_memory().unwrap();
    db.delete_preset("missing").unwrap();
}

#[test]
fn presets_survive_a_reopen() {
    let dir = tmp();
    let path = db_path(&dir);
    Db::open(&path, false)
        .unwrap()
        .0
        .save_preset("numbered", RULES, 100)
        .unwrap();
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(names(&db), ["numbered"]);
}
