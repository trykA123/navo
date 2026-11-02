use super::super::verbatim::verbatim;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn check(input: &str, expected: &str) {
    assert_eq!(
        String::from_utf16_lossy(&verbatim(&units(input))),
        expected,
        "{input}"
    );
}

#[test]
fn a_drive_path_gets_the_verbatim_prefix() {
    check(r"C:\a\b", r"\\?\C:\a\b");
}

#[test]
fn a_unc_path_gets_the_verbatim_unc_prefix() {
    check(r"\\srv\share\x", r"\\?\UNC\srv\share\x");
}

#[test]
fn forward_slashes_become_backslashes() {
    check("C:/a/b", r"\\?\C:\a\b");
}

#[test]
fn dot_segments_are_dropped() {
    check(r"C:\a\.\b", r"\\?\C:\a\b");
}

#[test]
fn dot_dot_removes_the_previous_segment() {
    check(r"C:\a\..\b", r"\\?\C:\b");
}

#[test]
fn dot_dot_never_climbs_above_the_root() {
    check(r"C:\..\..\b", r"\\?\C:\b");
    check(r"\\srv\share\..\x", r"\\?\UNC\srv\share\x");
}

#[test]
fn three_dots_is_a_name() {
    check(r"C:\a\...", r"\\?\C:\a\...");
}

#[test]
fn a_bare_drive_is_its_root() {
    check("C:", r"\\?\C:\");
    check(r"C:\", r"\\?\C:\");
}

#[test]
fn doubled_separators_collapse() {
    check(r"C:\a\\b\", r"\\?\C:\a\b");
}

#[test]
fn verbatim_and_device_paths_are_unchanged() {
    check(r"\\?\C:\a\..\b", r"\\?\C:\a\..\b");
    check(r"\\.\pipe\x", r"\\.\pipe\x");
}

#[test]
fn relative_and_rooted_paths_are_unchanged() {
    check(r"a\b", r"a\b");
    check(r"C:rel", r"C:rel");
    check(r"\a", r"\a");
}

#[test]
fn a_lone_surrogate_in_a_name_survives() {
    let mut input = units(r"C:\a\");
    input.push(0xD800);
    let mut expected = units(r"\\?\C:\a\");
    expected.push(0xD800);
    assert_eq!(verbatim(&input), expected);
}
