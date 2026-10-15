use super::*;

#[test]
fn add_folder_creates_missing_parents() {
    let fs = FakeFs::new();
    fs.add_folder("a/b/c");
    assert_eq!(
        fs.metadata(&fs.location("a/b")).map(|e| e.kind),
        Ok(EntryKind::Folder)
    );
}

#[test]
fn nested_folder_is_listed_in_its_parent() {
    let fs = FakeFs::new();
    fs.add_folder("a/b/c");
    assert_eq!(names(&list_all(&fs, "a/b").1), ["c"]);
}

#[test]
fn add_file_keeps_its_size() {
    let fs = FakeFs::new();
    fs.add_folder("a");
    fs.add_file("a/f.txt", 1234);
    assert_eq!(
        fs.metadata(&fs.location("a/f.txt")).map(|e| e.size),
        Ok(1234)
    );
}

#[test]
fn file_metadata_has_name_kind_and_size() {
    let fs = FakeFs::new();
    fs.add_folder("a");
    fs.add_file("a/f.txt", 7);
    let entry = fs.metadata(&fs.location("a/f.txt")).unwrap();
    assert_eq!(
        (entry.name.to_text(), entry.kind, entry.size),
        ("f.txt".to_owned(), EntryKind::File, 7)
    );
}

#[test]
fn folder_metadata_is_a_folder_of_size_zero() {
    let fs = FakeFs::new();
    fs.add_folder("a/b");
    let entry = fs.metadata(&fs.location("a/b")).unwrap();
    assert_eq!(
        (entry.name.to_text(), entry.kind, entry.size),
        ("b".to_owned(), EntryKind::Folder, 0)
    );
}

#[test]
fn root_metadata_is_a_folder() {
    let fs = FakeFs::new();
    assert_eq!(
        fs.metadata(&fs.location("")).map(|e| e.kind),
        Ok(EntryKind::Folder)
    );
}

#[test]
fn missing_path_is_not_found() {
    let fs = FakeFs::new();
    assert_eq!(
        kind_of(fs.metadata(&fs.location("nope"))),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn not_found_carries_the_location() {
    let fs = FakeFs::new();
    let location = fs.location("a/nope");
    assert_eq!(
        fs.metadata(&location).err().and_then(|e| e.location),
        Some(location)
    );
}

#[test]
fn metadata_of_another_provider_is_not_found() {
    let fs = FakeFs::new();
    fs.add_folder("a");
    let other = Location::new("other://a").unwrap();
    assert_eq!(kind_of(fs.metadata(&other)), Some(FsErrorKind::NotFound));
}

#[test]
fn removed_file_is_not_found() {
    let fs = FakeFs::new();
    fs.add_folder("a");
    fs.add_file("a/f", 1);
    fs.remove("a/f");
    assert_eq!(
        kind_of(fs.metadata(&fs.location("a/f"))),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn removed_folder_takes_its_children() {
    let fs = FakeFs::new();
    fs.add_folder("a/b");
    fs.add_file("a/b/f", 1);
    fs.remove("a");
    assert_eq!(
        kind_of(fs.metadata(&fs.location("a/b/f"))),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn removed_entry_leaves_the_listing() {
    let fs = with_files(3);
    fs.remove("d/f01");
    assert_eq!(names(&list_all(&fs, "d").1), ["f00", "f02"]);
}

fn round_trip(name: ProviderPath) -> Vec<ProviderPath> {
    let fs = FakeFs::new();
    fs.add_folder("h");
    fs.add_entry("h", file_entry(name));
    list_all(&fs, "h")
        .1
        .into_iter()
        .flat_map(|b| b.entries)
        .map(|e| e.name)
        .collect()
}

#[test]
fn lone_surrogate_name_round_trips() {
    let name = ProviderPath::from_wide(&[0x61, 0xD800, 0x62]);
    assert_eq!(round_trip(name.clone()), [name]);
}

#[test]
fn lone_surrogate_name_keeps_its_units() {
    let units = [0x61, 0xDC00];
    let back: Vec<_> = round_trip(ProviderPath::from_wide(&units))
        .iter()
        .map(ProviderPath::to_wide)
        .collect();
    assert_eq!(back, [Some(units.to_vec())]);
}

#[test]
fn invalid_utf8_name_keeps_its_bytes() {
    let bytes = [b'a', 0xFF, 0xC3, b'b'];
    let back: Vec<_> = round_trip(ProviderPath::from_bytes(&bytes))
        .iter()
        .map(ProviderPath::to_bytes)
        .collect();
    assert_eq!(back, [Some(bytes.to_vec())]);
}

#[test]
fn trailing_dots_and_spaces_survive() {
    let name = ProviderPath::from_text("name. . ");
    assert_eq!(round_trip(name.clone()), [name]);
}

#[test]
fn long_name_survives() {
    let name = ProviderPath::from_text(&"x".repeat(300));
    assert_eq!(round_trip(name.clone()), [name]);
}
