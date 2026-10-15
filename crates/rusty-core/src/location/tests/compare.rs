use super::*;

fn text(text: &str) -> ProviderPath {
    ProviderPath::from_text(text)
}

#[test]
fn windows_eq_ignores_ascii_case() {
    assert!(text(r"C:\Users\Admin").windows_eq(&text(r"c:\users\ADMIN")));
}

#[test]
fn windows_eq_does_not_expand_sharp_s() {
    assert!(!text("ß").windows_eq(&text("SS")));
}

#[test]
fn windows_eq_ignores_greek_case() {
    assert!(text("Σίσυφος").windows_eq(&text("ΣΊΣΥΦΟΣ")));
}

#[test]
fn windows_eq_ignores_cyrillic_case() {
    assert!(text("Привет ёж").windows_eq(&text("ПРИВЕТ ЁЖ")));
}

#[test]
fn windows_eq_keeps_supplementary_letters_case_sensitive() {
    assert!(!text("\u{10428}").windows_eq(&text("\u{10400}")));
}

#[test]
fn windows_eq_needs_equal_lengths() {
    assert!(!text("abc").windows_eq(&text("ABCD")));
}

#[test]
fn windows_eq_tells_different_letters_apart() {
    assert!(!text("abc").windows_eq(&text("abd")));
}

#[test]
fn windows_eq_keeps_lone_surrogates_as_they_are() {
    let lower = ProviderPath::from_wide(&[0x61, 0xD800]);
    let upper = ProviderPath::from_wide(&[0x41, 0xD800]);
    assert!(lower.windows_eq(&upper));
}

#[test]
fn drive_paths_in_other_case_are_the_same_place() {
    assert!(loc(r"C:\Users\Admin").same_place(&loc(r"c:\users\ADMIN")));
}

#[test]
fn unc_paths_in_other_case_are_the_same_place() {
    assert!(loc(r"\\srv\Share\X").same_place(&loc(r"\\SRV\share\x")));
}

#[test]
fn unix_paths_stay_case_sensitive() {
    assert!(!loc("/home/Admin").same_place(&loc("/home/admin")));
}

#[test]
fn equal_unix_paths_are_the_same_place() {
    assert!(loc("/home/admin").same_place(&loc("/home/admin")));
}

#[test]
fn other_providers_compare_exactly() {
    assert!(!loc(r"archive://C:\A.zip").same_place(&loc(r"archive://c:\a.zip")));
}

#[test]
fn different_providers_are_never_the_same_place() {
    assert!(!loc(r"archive://C:\a.zip").same_place(&loc(r"C:\a.zip")));
}
