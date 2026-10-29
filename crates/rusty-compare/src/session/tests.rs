mod fake;
mod memory;
mod robustness;

use std::sync::Arc;
use std::time::Duration;

use fake::FakeSource;
use rusty_core::ProviderPath;

use super::*;
use crate::model::{
    CaseRule, ContentClass, EntryKind, Generation, HashCache, ReadHint, Reason, RelPath, Row,
    SideEntry, Status,
};
use crate::rules::{IgnoreRules, IgnoreSource};

struct NoCache;

impl HashCache for NoCache {
    fn get(&self, _key: &crate::model::ContentKey) -> Option<[u8; 32]> {
        None
    }

    fn put(&self, _key: &crate::model::ContentKey, _hash: [u8; 32]) {}
}

fn options(mode: CheckMode) -> SessionOptions {
    let rules = IgnoreRules::new(Vec::<IgnoreSource>::new(), &[], false, CaseRule::Sensitive)
        .unwrap()
        .0;
    SessionOptions {
        rules: Arc::new(rules),
        cache: Arc::new(NoCache),
        read_hint: ReadHint::Buffered,
        check_mode: mode,
    }
}

fn start(left: FakeSource, right: FakeSource, mode: CheckMode) -> Session {
    Session::start(Arc::new(left), Arc::new(right), options(mode))
}

fn collect(session: &Session) -> Vec<RowBatch> {
    let mut batches = Vec::new();
    loop {
        let batch = session
            .batches()
            .recv_timeout(Duration::from_secs(20))
            .expect("session should emit a batch");
        let is_final = batch.is_final;
        batches.push(batch);
        if is_final {
            return batches;
        }
    }
}

fn row_name(row: &Row) -> String {
    row.left
        .as_deref()
        .or(row.right.as_deref())
        .map(|entry| entry.rel.as_path().to_text())
        .unwrap_or_default()
}

fn batches_with_row<'a>(
    batches: &'a [RowBatch],
    name: &'a str,
) -> impl Iterator<Item = (usize, &'a Row)> + 'a {
    batches.iter().enumerate().flat_map(move |(index, batch)| {
        batch
            .rows
            .iter()
            .filter(move |row| row_name(row) == name)
            .map(move |row| (index, row))
    })
}

#[test]
fn batches_arrive_structure_first_and_final_last() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for source in [&mut left, &mut right] {
        source.add_folder("sub");
        source.add_file("same.txt", b"same".to_vec());
        source.add_file("sub/child.txt", b"child".to_vec());
    }
    left.add_file("diff.txt", b"aaaa".to_vec());
    right.add_file("diff.txt", b"bbbb".to_vec());
    let session = start(left, right, CheckMode::Content);
    let generation = session.generation();
    let batches = collect(&session);

    assert!(batches.iter().all(|batch| batch.generation == generation));
    let first = batches.first().expect("at least one batch");
    assert!(!first.is_final);
    assert!(first.totals.checking > 0);

    let final_batch = batches.last().expect("final batch");
    assert!(final_batch.is_final);
    assert_eq!(final_batch.totals.checking, 0);
    assert_eq!(final_batch.totals.rows, 4);

    let parent = batches_with_row(&batches, "sub")
        .next()
        .expect("folder row");
    let child = batches_with_row(&batches, "sub/child.txt")
        .next()
        .expect("child row");
    assert!(parent.0 < child.0);

    for name in ["same.txt", "diff.txt", "sub/child.txt"] {
        let mut seen_checking = false;
        let mut seen_final = false;
        for (_, row) in batches_with_row(&batches, name) {
            if row.raw_status == Status::Checking {
                seen_checking = true;
            } else if seen_checking {
                seen_final = true;
            }
        }
        assert!(seen_checking, "{name} was never checking");
        assert!(seen_final, "{name} never reached a final status");
    }

    let different = batches_with_row(&batches, "diff.txt")
        .last()
        .expect("diff row");
    assert!(matches!(different.1.raw_status, Status::Different { .. }));
}

#[test]
fn totals_stay_unfinal_while_a_row_is_checking() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("a.txt", b"data".to_vec());
    right.add_file("a.txt", b"data".to_vec());
    let session = start(left, right, CheckMode::Content);

    let first = session
        .batches()
        .recv_timeout(Duration::from_secs(20))
        .expect("initial batch");
    assert_eq!(first.totals.checking, 1);
    assert!(!first.is_final);

    let batches = collect(&session);
    let final_batch = batches.last().expect("final batch");
    assert!(final_batch.is_final);
    assert_eq!(final_batch.totals.checking, 0);
    assert_eq!(final_batch.totals.same, 1);
}

#[test]
fn refresh_bumps_the_generation_and_drops_old_batches() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for index in 0..32 {
        let name = format!("f{index:02}.txt");
        left.add_file(&name, b"same".to_vec());
        right.add_file(&name, b"same".to_vec());
    }
    let mut session = start(left, right, CheckMode::Content);
    let old_generation = session.generation();
    let first = session
        .batches()
        .recv_timeout(Duration::from_secs(20))
        .expect("initial batch");
    let old_row = first.rows.first().expect("initial row").clone();

    session.refresh();
    let new_generation = session.generation();
    assert!(new_generation > old_generation);

    let batches = collect(&session);
    assert!(
        batches
            .iter()
            .all(|batch| batch.generation == new_generation)
    );
    assert_eq!(
        session.write_allowed(&old_row, old_generation),
        Err(Refusal::StaleGeneration)
    );
}

#[test]
fn write_allowed_returns_each_d5_refusal() {
    let session = start(
        FakeSource::new(true),
        FakeSource::new(true),
        CheckMode::SizeTime,
    );
    let generation = session.generation();

    let stale = generation_with(Generation::new(0), Status::Same, EntryKind::File);
    assert_eq!(
        session.write_allowed(&stale, generation),
        Err(Refusal::StaleGeneration)
    );

    let checking = generation_with(generation, Status::Checking, EntryKind::File);
    assert_eq!(
        session.write_allowed(&checking, generation),
        Err(Refusal::Checking)
    );

    let link = generation_with(generation, Status::Same, EntryKind::Link);
    assert_eq!(session.write_allowed(&link, generation), Err(Refusal::Link));

    let submodule = generation_with(generation, Status::Same, EntryKind::Submodule);
    assert_eq!(
        session.write_allowed(&submodule, generation),
        Err(Refusal::Submodule)
    );

    let unavailable = generation_with(
        generation,
        Status::Unavailable(Reason::NotFound),
        EntryKind::File,
    );
    assert_eq!(
        session.write_allowed(&unavailable, generation),
        Err(Refusal::Unavailable(Reason::NotFound))
    );

    let allowed = generation_with(generation, Status::Same, EntryKind::File);
    assert_eq!(session.write_allowed(&allowed, generation), Ok(()));
}

#[test]
fn ignored_files_and_folders_are_not_listed_or_entered() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for source in [&mut left, &mut right] {
        source.add_file("keep.txt", b"keep".to_vec());
        source.add_file("skip.tmp", b"skip".to_vec());
        source.add_folder("secret");
        source.add_file("secret/child.txt", b"child".to_vec());
    }
    let rules = IgnoreRules::new(
        vec![IgnoreSource::Gitignore {
            folder: RelPath::root(),
            contents: "*.tmp\nsecret/\n".to_owned(),
        }],
        &[],
        false,
        CaseRule::Sensitive,
    )
    .unwrap()
    .0;
    let options = SessionOptions {
        rules: Arc::new(rules),
        cache: Arc::new(NoCache),
        read_hint: ReadHint::Buffered,
        check_mode: CheckMode::Content,
    };
    let session = Session::start(Arc::new(left), Arc::new(right), options);
    let batches = collect(&session);

    let names: Vec<String> = batches
        .iter()
        .flat_map(|batch| batch.rows.iter())
        .map(row_name)
        .collect();
    assert!(names.iter().any(|name| name == "keep.txt"));
    assert!(
        !names
            .iter()
            .any(|name| name == "skip.tmp" || name == "secret" || name == "secret/child.txt")
    );
}

fn generation_with(generation: Generation, status: Status, kind: EntryKind) -> Row {
    let entry = Arc::new(SideEntry::new(
        RelPath::new(ProviderPath::from_text("a")),
        kind,
        0,
        None,
    ));
    Row {
        left: Some(Arc::clone(&entry)),
        right: Some(entry),
        raw_status: status,
        display_status: status,
        generation,
        content: ContentClass::Unknown,
    }
}
