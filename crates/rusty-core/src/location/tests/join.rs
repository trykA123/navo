use super::*;

fn name(text: &str) -> ProviderPath {
    ProviderPath::from_text(text)
}

fn fake(path: &str) -> Location {
    Location::with_provider(ProviderId::new("fake").unwrap(), name(path)).unwrap()
}

#[test]
fn windows_paths_join_with_a_backslash() {
    assert_eq!(
        loc(r"C:\Users").join(&name("admin")).unwrap().as_str(),
        r"C:\Users\admin"
    );
}

#[test]
fn a_drive_root_gets_no_second_backslash() {
    assert_eq!(loc(r"C:\").join(&name("x")).unwrap().as_str(), r"C:\x");
}

#[test]
fn a_bare_drive_joins_from_its_root() {
    assert_eq!(loc("C:").join(&name("x")).unwrap().as_str(), r"C:\x");
}

#[test]
fn unc_paths_join_with_a_backslash() {
    assert_eq!(
        loc(r"\\srv\share").join(&name("x")).unwrap().as_str(),
        r"\\srv\share\x"
    );
}

#[test]
fn unix_paths_join_with_a_slash() {
    assert_eq!(
        loc("/home").join(&name("admin")).unwrap().as_str(),
        "/home/admin"
    );
    assert_eq!(loc("/").join(&name("etc")).unwrap().as_str(), "/etc");
}

#[test]
fn other_providers_join_with_a_slash() {
    assert_eq!(
        fake("a/b").join(&name("c")).unwrap().as_str(),
        "fake://a/b/c"
    );
    assert_eq!(fake("/").join(&name("c")).unwrap().as_str(), "fake:///c");
}

#[test]
fn the_provider_is_kept() {
    let joined = fake("a").join(&name("b")).unwrap();
    assert_eq!(joined.provider().as_str(), "fake");
}

#[test]
fn a_lone_surrogate_name_joins_losslessly() {
    let odd = ProviderPath::from_wide(&[0x61, 0xD800]);
    let joined = loc(r"C:\d").join(&odd).unwrap();
    let mut expected: Vec<u16> = r"C:\d\".encode_utf16().collect();
    expected.extend([0x61, 0xD800]);
    assert_eq!(joined.path().to_wide(), Some(expected));
}

#[test]
fn an_invalid_utf8_name_joins_losslessly() {
    let odd = ProviderPath::from_bytes(b"caf\xe9");
    let joined = loc("/srv").join(&odd).unwrap();
    assert_eq!(joined.path().to_bytes(), Some(b"/srv/caf\xe9".to_vec()));
}

#[test]
fn a_wide_name_under_a_bytes_folder_is_refused() {
    let folder = Location::local(ProviderPath::from_bytes(b"/x\xff")).unwrap();
    let wide = ProviderPath::from_wide(&[0xD800]);
    assert_eq!(folder.join(&wide), Err(LocationError::BadSegment));
}

#[test]
fn empty_names_are_refused() {
    assert_eq!(loc("/a").join(&name("")), Err(LocationError::Empty));
}

#[test]
fn nul_in_a_name_is_refused() {
    assert_eq!(loc("/a").join(&name("b\0c")), Err(LocationError::Nul));
}

#[test]
fn dot_names_are_refused() {
    assert_eq!(loc("/a").join(&name(".")), Err(LocationError::BadSegment));
    assert_eq!(loc("/a").join(&name("..")), Err(LocationError::BadSegment));
}

#[test]
fn a_slash_in_a_name_is_refused() {
    assert_eq!(loc("/a").join(&name("b/c")), Err(LocationError::BadSegment));
    assert_eq!(fake("a").join(&name("b/c")), Err(LocationError::BadSegment));
}

#[test]
fn a_backslash_is_a_separator_only_on_windows_paths() {
    assert_eq!(
        loc(r"C:\a").join(&name(r"b\c")),
        Err(LocationError::BadSegment)
    );
    assert_eq!(
        loc(r"C:\a").join(&name("b/c")),
        Err(LocationError::BadSegment)
    );
    assert_eq!(loc("/a").join(&name(r"b\c")).unwrap().as_str(), r"/a/b\c");
}

#[test]
fn a_wide_name_with_a_backslash_is_refused_on_windows() {
    let odd = ProviderPath::from_wide(&[0x61, 0x5C, 0xD800]);
    assert_eq!(loc(r"C:\a").join(&odd), Err(LocationError::BadSegment));
}

fn segment() -> impl proptest::strategy::Strategy<Value = ProviderPath> {
    use proptest::prelude::*;
    let unit = (1..=u16::MAX).prop_filter("no separator", |&u| u != 0x2F && u != 0x5C);
    prop::collection::vec(unit, 1..12)
        .prop_map(|units| ProviderPath::from_wide(&units))
        .prop_filter("no dot names", |p| !p.is_dot_name())
}

proptest::proptest! {
    #[test]
    fn joined_windows_locations_survive_their_text_form(name in segment()) {
        let joined = loc(r"C:\d").join(&name).unwrap();
        proptest::prop_assert_eq!(Location::new(joined.as_str()), Ok(joined));
    }
}
