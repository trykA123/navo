use super::*;

fn parent(text: &str) -> Option<String> {
    loc(text).parent().map(|p| p.as_str().to_owned())
}

#[test]
fn a_drive_folder_goes_up_one_level() {
    assert_eq!(parent(r"C:\Users\admin").as_deref(), Some(r"C:\Users"));
}

#[test]
fn a_top_folder_goes_up_to_the_drive_root() {
    assert_eq!(parent(r"C:\Users").as_deref(), Some(r"C:\"));
}

#[test]
fn a_drive_root_has_no_parent() {
    assert_eq!(parent(r"C:\"), None);
    assert_eq!(parent("C:"), None);
}

#[test]
fn a_trailing_separator_is_ignored() {
    assert_eq!(parent(r"C:\Users\admin\").as_deref(), Some(r"C:\Users"));
}

#[test]
fn a_unc_folder_goes_up_to_its_share() {
    assert_eq!(parent(r"\\srv\share\x").as_deref(), Some(r"\\srv\share"));
}

#[test]
fn a_unc_share_has_no_parent() {
    assert_eq!(parent(r"\\srv\share"), None);
}

#[test]
fn a_verbatim_drive_path_stops_at_its_root() {
    assert_eq!(parent(r"\\?\C:\a").as_deref(), Some(r"\\?\C:\"));
    assert_eq!(parent(r"\\?\C:\"), None);
}

#[test]
fn a_unix_folder_goes_up() {
    assert_eq!(parent("/home/admin").as_deref(), Some("/home"));
    assert_eq!(parent("/home").as_deref(), Some("/"));
    assert_eq!(parent("/"), None);
}

#[test]
fn a_provider_folder_goes_up() {
    assert_eq!(parent("fake://a/b").as_deref(), Some("fake://a"));
    assert_eq!(parent("fake://a").as_deref(), Some("fake:///"));
    assert_eq!(parent("fake:///"), None);
}

#[test]
fn join_then_parent_returns_the_folder() {
    let folder = loc(r"C:\data");
    let child = folder
        .join(&ProviderPath::from_wide(&[0x61, 0xD800]))
        .unwrap();
    assert_eq!(child.parent(), Some(folder));
}

#[test]
fn name_is_the_last_segment() {
    assert_eq!(
        loc(r"C:\Users\admin").name(),
        Some(ProviderPath::from_text("admin"))
    );
    assert_eq!(loc("fake://a/b").name(), Some(ProviderPath::from_text("b")));
    assert_eq!(loc("/home").name(), Some(ProviderPath::from_text("home")));
}

#[test]
fn a_root_has_no_name() {
    assert_eq!(loc(r"C:\").name(), None);
    assert_eq!(loc("/").name(), None);
}

#[test]
fn name_keeps_a_lone_surrogate() {
    let odd = ProviderPath::from_wide(&[0x61, 0xD800]);
    let child = loc(r"C:\d").join(&odd).unwrap();
    assert_eq!(child.name(), Some(odd));
}
