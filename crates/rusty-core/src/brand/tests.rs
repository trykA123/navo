use super::{is_part_name, is_rename_temp};

#[test]
fn part_names_accept_both_suffixes() {
    assert!(is_part_name("report.txt.navo-part"));
    assert!(is_part_name("report.txt.ferrite-part"));
    assert!(!is_part_name("report.txt"));
    assert!(!is_part_name("report.navo-part.txt"));
}

#[test]
fn rename_temps_accept_both_prefixes() {
    assert!(is_rename_temp(".navo-rename-00000007-0"));
    assert!(is_rename_temp(".ferrite-rename-00000007-0"));
    assert!(!is_rename_temp("navo-rename-7"));
    assert!(!is_rename_temp("rename-.navo-rename-7"));
}
