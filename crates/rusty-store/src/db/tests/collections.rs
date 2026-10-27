use super::*;

fn create(db: &Db, name: &str, kind: CollectionKind) -> i64 {
    db.create_collection(
        &NewCollection {
            name,
            kind,
            query: Some("ext:rs"),
            base: Some("/work"),
        },
        100,
    )
    .unwrap()
}

#[test]
fn create_and_list_preserve_all_collection_fields() {
    let db = Db::open_in_memory().unwrap();
    assert!(db.list_collections().unwrap().is_empty());
    let id = create(&db, "Sources", CollectionKind::Search);
    assert_eq!(
        db.list_collections().unwrap(),
        [Collection {
            id,
            name: "Sources".into(),
            kind: CollectionKind::Search,
            query: Some("ext:rs".into()),
            base: Some("/work".into()),
            created_ms: 100,
            updated_ms: 100,
        }]
    );
}

#[test]
fn names_are_unique_and_between_one_and_sixty_characters() {
    let db = Db::open_in_memory().unwrap();
    create(&db, "Sources", CollectionKind::Snapshot);
    for name in ["", " ", &"é".repeat(61)] {
        assert!(matches!(
            db.create_collection(
                &NewCollection {
                    name,
                    kind: CollectionKind::Manual,
                    query: None,
                    base: None
                },
                1,
            ),
            Err(DbError::InvalidCollectionName)
        ));
    }
    create(&db, &"é".repeat(60), CollectionKind::Manual);
    assert!(
        db.create_collection(
            &NewCollection {
                name: "Sources",
                kind: CollectionKind::Manual,
                query: None,
                base: None
            },
            1,
        )
        .is_err()
    );
}

#[test]
fn rename_validates_names_and_updates_only_the_requested_collection() {
    let db = Db::open_in_memory().unwrap();
    let id = create(&db, "Sources", CollectionKind::Search);
    create(&db, "Other", CollectionKind::Manual);
    assert!(db.rename_collection(id, "Other", 200).is_err());
    assert!(matches!(
        db.rename_collection(id, "", 200),
        Err(DbError::InvalidCollectionName)
    ));
    db.rename_collection(id, "Renamed", 200).unwrap();
    let rows = db.list_collections().unwrap();
    let row = rows.iter().find(|row| row.id == id).unwrap();
    assert_eq!(
        (&*row.name, row.created_ms, row.updated_ms),
        ("Renamed", 100, 200)
    );
}

#[test]
fn adding_twice_returns_only_new_locations_for_undo() {
    let db = Db::open_in_memory().unwrap();
    let id = create(&db, "Files", CollectionKind::Manual);
    assert_eq!(
        db.add_collection_items(id, &["/missing", "/missing"], 110)
            .unwrap(),
        ["/missing"]
    );
    let added = db
        .add_collection_items(id, &["/missing", "/new"], 120)
        .unwrap();
    assert_eq!(added, ["/new"]);
    db.remove_collection_items(id, &added, 130).unwrap();
    assert_eq!(
        db.collection_items(id).unwrap(),
        [CollectionItem {
            location: "/missing".into(),
            added_ms: 110
        }]
    );
    assert_eq!(db.list_collections().unwrap()[0].updated_ms, 130);
}

#[test]
fn delete_cascades_only_its_own_items() {
    let db = Db::open_in_memory().unwrap();
    let first = create(&db, "First", CollectionKind::Snapshot);
    let second = create(&db, "Second", CollectionKind::Manual);
    for id in [first, second] {
        db.add_collection_items(id, &["/missing"], 110).unwrap();
    }
    db.delete_collection(first).unwrap();
    assert!(db.collection_items(first).unwrap().is_empty());
    assert_eq!(db.collection_items(second).unwrap().len(), 1);
    assert_eq!(db.list_collections().unwrap().len(), 1);
    assert!(db.add_collection_items(first, &["/orphan"], 120).is_err());
}

#[test]
fn collections_and_missing_locations_survive_reopening() {
    let dir = tmp();
    let path = db_path(&dir);
    let id = {
        let (db, _) = Db::open(&path, false).unwrap();
        let id = create(&db, "Files", CollectionKind::Snapshot);
        db.add_collection_items(id, &["/missing"], 110).unwrap();
        id
    };
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(db.list_collections().unwrap()[0].id, id);
    assert_eq!(db.collection_items(id).unwrap()[0].location, "/missing");
}

#[test]
fn migration_from_v7_preserves_existing_data_and_enables_collections() {
    let dir = tmp();
    let path = db_path(&dir);
    build_at_version(&path, 7);
    Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO rename_preset VALUES ('existing', '{}', 42)",
            [],
        )
        .unwrap();
    let (db, report) = Db::open(&path, false).unwrap();
    assert_eq!((report.from_version, report.to_version), (7, 8));
    assert_eq!(db.presets().unwrap().named[0].name, "existing");
    assert!(db.list_collections().unwrap().is_empty());
    let id = create(&db, "Migrated", CollectionKind::Manual);
    db.add_collection_items(id, &["/missing"], 110).unwrap();
    assert_eq!(db.collection_items(id).unwrap().len(), 1);
}
