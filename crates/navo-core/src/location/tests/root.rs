use super::*;

fn root(text: &str) -> Location {
    loc(text).root()
}

#[test]
fn a_drive_folder_has_its_drive_root() {
    assert_eq!(root(r"C:\Users\admin\x"), loc(r"C:\"));
}

#[test]
fn a_drive_root_keeps_its_letter_case() {
    assert_eq!(root(r"c:\").as_str(), r"c:\");
}

#[test]
fn a_bare_drive_gets_a_separator() {
    assert_eq!(root("C:"), loc(r"C:\"));
}

#[test]
fn a_unc_folder_has_its_share_root() {
    assert_eq!(root(r"\\nas\media\films\a"), loc(r"\\nas\media\"));
}

#[test]
fn a_bare_unc_share_gets_a_separator() {
    assert_eq!(root(r"\\nas\media"), loc(r"\\nas\media\"));
}

#[test]
fn a_verbatim_drive_path_has_its_verbatim_root() {
    assert_eq!(root(r"\\?\C:\long\path"), loc(r"\\?\C:\"));
}

#[test]
fn a_verbatim_unc_path_has_its_verbatim_share_root() {
    assert_eq!(root(r"\\?\UNC\nas\media\x"), loc(r"\\?\UNC\nas\media\"));
}

#[test]
fn a_unix_folder_has_the_slash_root() {
    assert_eq!(root("/home/admin"), loc("/"));
}

#[test]
fn another_provider_has_its_provider_root() {
    let fake = ProviderId::new("fake").unwrap();
    assert_eq!(root("fake://home/a/b"), Location::provider_root(fake));
}

#[test]
fn the_root_of_a_root_is_itself() {
    let samples = [
        r"C:\Users\admin\x",
        "C:",
        r"\\nas\media\films",
        r"\\nas\media",
        r"\\?\C:\long\path",
        r"\\?\UNC\nas\media\x",
        "/home/admin",
        "fake://home/a/b",
    ];
    let unstable: Vec<_> = samples
        .iter()
        .filter(|text| root(text).root() != root(text))
        .collect();
    assert_eq!(unstable, Vec::<&&str>::new());
}
