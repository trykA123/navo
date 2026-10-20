use super::*;

const NOW: i64 = 1_760_900_000_000;
const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

fn count(db: &Db, table: &str) -> i64 {
    db.connection()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn score(db: &Db, location: &str) -> Option<f64> {
    db.connection()
        .query_row(
            "SELECT score FROM frecency WHERE location = ?1",
            [location],
            |row| row.get(0),
        )
        .ok()
}

fn seed(db: &Db, table: &str, rows: &[(&str, f64)]) {
    let key_column = if table == "frecency" {
        "location"
    } else {
        "name"
    };
    for (key, score) in rows {
        db.connection()
            .execute(
                &format!("INSERT INTO {table} ({key_column}, score, last_at) VALUES (?1, ?2, ?3)"),
                rusqlite::params![key, score, NOW],
            )
            .unwrap();
    }
}

fn weight_after(age: i64) -> f64 {
    let db = Db::open_in_memory().unwrap();
    db.record_visit("file:///a", true, NOW - age).unwrap();
    db.frecent(1, NOW).unwrap()[0].weight
}

#[test]
fn a_first_visit_scores_one() {
    let db = Db::open_in_memory().unwrap();
    db.record_visit("file:///a", true, NOW).unwrap();
    assert_eq!(score(&db, "file:///a"), Some(1.0));
}

#[test]
fn each_visit_adds_one() {
    let db = Db::open_in_memory().unwrap();
    for _ in 0..3 {
        db.record_visit("file:///a", true, NOW).unwrap();
    }
    assert_eq!(score(&db, "file:///a"), Some(3.0));
}

#[test]
fn a_visit_is_added_to_history() {
    let db = Db::open_in_memory().unwrap();
    db.record_visit("file:///a", true, NOW).unwrap();
    let row: (String, i64) = db
        .connection()
        .query_row("SELECT location, visited_at FROM history", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(row, ("file:///a".to_owned(), NOW));
}

#[test]
fn a_file_visit_is_listed_as_a_file() {
    let db = Db::open_in_memory().unwrap();
    db.record_visit("file:///a.txt", false, NOW).unwrap();
    assert!(!db.frecent(1, NOW).unwrap()[0].is_folder);
}

#[test]
fn a_use_within_the_hour_weighs_four_times() {
    assert_eq!(weight_after(59 * MINUTE), 4.0);
}

#[test]
fn a_use_within_the_day_weighs_twice() {
    assert_eq!(weight_after(HOUR), 2.0);
}

#[test]
fn a_use_within_the_week_weighs_half() {
    assert_eq!(weight_after(DAY), 0.5);
}

#[test]
fn an_older_use_weighs_a_quarter() {
    assert_eq!(weight_after(7 * DAY), 0.25);
}

#[test]
fn frecent_lists_the_heaviest_first() {
    let db = Db::open_in_memory().unwrap();
    db.record_visit("file:///old", true, NOW - 2 * DAY).unwrap();
    db.record_visit("file:///old", true, NOW - 2 * DAY).unwrap();
    db.record_visit("file:///new", false, NOW).unwrap();
    let want = vec![
        Frecent {
            location: "file:///new".to_owned(),
            is_folder: false,
            weight: 4.0,
        },
        Frecent {
            location: "file:///old".to_owned(),
            is_folder: true,
            weight: 1.0,
        },
    ];
    assert_eq!(db.frecent(10, NOW).unwrap(), want);
}

#[test]
fn frecent_stops_at_the_limit() {
    let db = Db::open_in_memory().unwrap();
    for location in ["file:///a", "file:///b", "file:///c"] {
        db.record_visit(location, true, NOW).unwrap();
    }
    assert_eq!(db.frecent(2, NOW).unwrap().len(), 2);
}

#[test]
fn passing_ten_thousand_scales_every_score_by_nine_tenths() {
    let db = Db::open_in_memory().unwrap();
    seed(
        &db,
        "frecency",
        &[("file:///a", 9_000.0), ("file:///b", 1_000.0)],
    );
    db.record_visit("file:///a", true, NOW).unwrap();
    assert_eq!(
        (score(&db, "file:///a"), score(&db, "file:///b")),
        (Some(9_000.0 * 0.9 + 1.0), Some(900.0))
    );
}

#[test]
fn decay_drops_scores_below_one() {
    let db = Db::open_in_memory().unwrap();
    seed(
        &db,
        "frecency",
        &[("file:///a", 9_999.0), ("file:///b", 1.0)],
    );
    db.record_visit("file:///a", true, NOW).unwrap();
    assert_eq!(score(&db, "file:///b"), None);
}

#[test]
fn a_new_folder_recorded_at_the_threshold_keeps_its_first_weight() {
    let db = Db::open_in_memory().unwrap();
    seed(
        &db,
        "frecency",
        &[("file:///a", 9_999.0), ("file:///b", 1.0)],
    );
    db.record_visit("file:///new", true, NOW).unwrap();
    assert_eq!(score(&db, "file:///new"), Some(1.0));
}

#[test]
fn a_new_command_recorded_at_the_threshold_keeps_its_first_weight() {
    let db = Db::open_in_memory().unwrap();
    seed(&db, "command_use", &[("a", 9_999.0), ("b", 1.0)]);
    db.record_command("new", NOW).unwrap();
    let new = db
        .command_use(10, NOW)
        .unwrap()
        .into_iter()
        .find(|(name, _)| name == "new");
    assert_eq!(new, Some(("new".to_owned(), 4.0)));
}

#[test]
fn decay_at_the_threshold_still_trims_other_low_rows() {
    let db = Db::open_in_memory().unwrap();
    seed(
        &db,
        "frecency",
        &[("file:///a", 9_999.0), ("file:///b", 1.0)],
    );
    db.record_visit("file:///new", true, NOW).unwrap();
    assert_eq!(
        (score(&db, "file:///a"), score(&db, "file:///b")),
        (Some(9_999.0 * 0.9), None)
    );
}

#[test]
fn reaching_exactly_ten_thousand_does_not_decay() {
    let db = Db::open_in_memory().unwrap();
    seed(&db, "frecency", &[("file:///a", 9_999.0)]);
    db.record_visit("file:///b", true, NOW).unwrap();
    assert_eq!(score(&db, "file:///a"), Some(9_999.0));
}

#[test]
fn history_keeps_the_newest_ten_thousand_visits() {
    let db = Db::open_in_memory().unwrap();
    db.connection()
        .execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 10000)
             INSERT INTO history (location, visited_at) SELECT printf('file:///%05d', i), i FROM n;",
        )
        .unwrap();
    db.record_visit("file:///newest", true, NOW).unwrap();
    let oldest: String = db
        .connection()
        .query_row(
            "SELECT location FROM history ORDER BY id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        (count(&db, "history"), oldest.as_str()),
        (10_000, "file:///00002")
    );
}

#[test]
fn a_failed_visit_changes_nothing() {
    let db = Db::open_in_memory().unwrap();
    db.record_visit("file:///a", true, NOW).unwrap();
    db.connection()
        .execute_batch(
            "CREATE TEMP TRIGGER refuse BEFORE INSERT ON history
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    assert!(db.record_visit("file:///a", true, NOW).is_err());
    assert_eq!(score(&db, "file:///a"), Some(1.0));
}

#[test]
fn record_command_adds_one_per_use() {
    let db = Db::open_in_memory().unwrap();
    db.record_command("view.toggle_hidden", NOW).unwrap();
    db.record_command("view.toggle_hidden", NOW).unwrap();
    assert_eq!(
        db.command_use(10, NOW).unwrap(),
        [("view.toggle_hidden".to_owned(), 8.0)]
    );
}

#[test]
fn command_use_lists_the_heaviest_first_up_to_the_limit() {
    let db = Db::open_in_memory().unwrap();
    db.record_command("old", NOW - 2 * DAY).unwrap();
    db.record_command("new", NOW).unwrap();
    db.record_command("older", NOW - 30 * DAY).unwrap();
    assert_eq!(
        db.command_use(2, NOW).unwrap(),
        [("new".to_owned(), 4.0), ("old".to_owned(), 0.5)]
    );
}

#[test]
fn command_use_decays_on_its_own_total() {
    let db = Db::open_in_memory().unwrap();
    seed(&db, "frecency", &[("file:///a", 9_999.0)]);
    seed(&db, "command_use", &[("a", 9_999.0), ("b", 1.0)]);
    db.record_command("a", NOW).unwrap();
    let names: Vec<String> = db
        .command_use(10, NOW)
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        (names, score(&db, "file:///a")),
        (vec!["a".to_owned()], Some(9_999.0))
    );
}

#[test]
fn clear_search_history_empties_history_frecency_and_command_use() {
    let db = Db::open_in_memory().unwrap();
    db.record_visit("file:///a", true, NOW).unwrap();
    db.record_command("a", NOW).unwrap();
    db.clear_search_history().unwrap();
    let counts: Vec<i64> = ["history", "frecency", "command_use"]
        .iter()
        .map(|table| count(&db, table))
        .collect();
    assert_eq!(counts, [0, 0, 0]);
}

#[test]
fn clear_search_history_keeps_every_other_table() {
    let dir = tmp();
    let path = db_path(&dir);
    build_from_sql(&path, V3_SQL);
    let (db, _) = Db::open(&path, false).unwrap();
    let others = [
        "job",
        "job_step",
        "module_kv",
        "search_history",
        "settings_defaults",
        "tags",
        "undo",
        "viewer_memory",
    ];
    let before: Vec<i64> = others.iter().map(|table| count(&db, table)).collect();
    db.clear_search_history().unwrap();
    let after: Vec<i64> = others.iter().map(|table| count(&db, table)).collect();
    assert_eq!(after, before);
}
