use super::*;

#[test]
fn windows_path_text_is_unchanged() {
    assert_eq!(loc(r"C:\Users\admin").as_str(), r"C:\Users\admin");
}

#[test]
fn unc_path_text_is_unchanged() {
    assert_eq!(loc(r"\\srv\share\x").as_str(), r"\\srv\share\x");
}

#[test]
fn unix_path_text_is_unchanged() {
    assert_eq!(loc("/home/admin").as_str(), "/home/admin");
}

#[test]
fn plain_paths_are_local() {
    let local: Vec<bool> = [r"C:\Users\admin", r"\\srv\share\x", "/home/admin"]
        .into_iter()
        .map(|text| loc(text).is_local())
        .collect();
    assert_eq!(local, [true, true, true]);
}

#[test]
fn local_provider_is_named_local() {
    assert_eq!(ProviderId::local().as_str(), "local");
}

#[test]
fn local_location_has_the_local_provider() {
    assert_eq!(loc("/home/admin").provider(), &ProviderId::local());
}

#[test]
fn local_location_keeps_its_path() {
    assert_eq!(loc("/home/admin").path().as_str(), Some("/home/admin"));
}

#[test]
fn provider_prefix_is_parsed() {
    assert_eq!(loc(r"archive://C:\a.zip").provider().as_str(), "archive");
}

#[test]
fn provider_prefix_is_not_part_of_the_path() {
    assert_eq!(
        loc(r"archive://C:\a.zip").path().as_str(),
        Some(r"C:\a.zip")
    );
}

#[test]
fn provider_text_form_is_unchanged() {
    assert_eq!(loc(r"archive://C:\a.zip").as_str(), r"archive://C:\a.zip");
}

#[test]
fn provider_location_is_not_local() {
    assert!(!loc(r"archive://C:\a.zip").is_local());
}

#[test]
fn with_provider_builds_the_text_form() {
    let archive = ProviderId::new("archive").unwrap();
    let location = Location::with_provider(archive, ProviderPath::from_text(r"C:\a.zip"));
    assert_eq!(location, Location::new(r"archive://C:\a.zip"));
}

#[test]
fn local_builds_the_same_location_as_new() {
    let location = Location::local(ProviderPath::from_text(r"C:\Users\admin"));
    assert_eq!(location, Location::new(r"C:\Users\admin"));
}

#[test]
fn explicit_local_prefix_is_refused() {
    assert_eq!(Location::new("local://x"), Err(LocationError::BadProvider));
}

#[test]
fn local_is_a_valid_provider_id() {
    assert_eq!(ProviderId::new("local"), Ok(ProviderId::local()));
}

#[test]
fn valid_provider_ids_are_accepted() {
    let ids = [
        "ab",
        "archive",
        "s3",
        "web-dav",
        "a0123456789012345678901234567890",
    ];
    let refused: Vec<&str> = ids
        .into_iter()
        .filter(|id| ProviderId::new(id).is_err())
        .collect();
    assert_eq!(refused, Vec::<&str>::new());
}

#[test]
fn bad_provider_ids_are_refused() {
    let ids = [
        "",
        "a",
        "Archive",
        "0zip",
        "-zip",
        "web_dav",
        "zip:",
        "a01234567890123456789012345678901",
    ];
    let accepted: Vec<&str> = ids
        .into_iter()
        .filter(|id| ProviderId::new(id) != Err(LocationError::BadProvider))
        .collect();
    assert_eq!(accepted, Vec::<&str>::new());
}

#[test]
fn prefix_with_a_bad_provider_id_is_a_local_path() {
    assert!(loc("Archive://x").is_local());
}

#[test]
fn drive_letter_is_not_a_provider() {
    assert!(loc(r"C:\x").is_local());
}

#[test]
fn single_letter_prefix_is_not_a_provider() {
    assert!(loc("c://x").is_local());
}

#[test]
fn provider_with_empty_path_is_empty() {
    assert_eq!(Location::new("archive://"), Err(LocationError::Empty));
}

#[test]
fn local_with_empty_path_is_empty() {
    assert_eq!(
        Location::local(ProviderPath::from_text("")),
        Err(LocationError::Empty)
    );
}

#[test]
fn nul_in_provider_path_is_refused() {
    assert_eq!(Location::new("archive://a\0b"), Err(LocationError::Nul));
}

#[test]
fn nul_in_wide_path_is_refused() {
    let path = ProviderPath::from_wide(&[0x61, 0, 0xD800]);
    assert_eq!(Location::local(path), Err(LocationError::Nul));
}

#[test]
fn nul_in_byte_path_is_refused() {
    let path = ProviderPath::from_bytes(&[0x61, 0, 0xFF]);
    assert_eq!(Location::local(path), Err(LocationError::Nul));
}
