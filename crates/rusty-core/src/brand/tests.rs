use super::{is_part_name, is_rename_temp, is_reserved_name, is_retained_container};

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

#[test]
fn retained_containers_accept_both_brands_in_any_case() {
    for name in [
        ".navo-retained-ab",
        ".ferrite-retained-ab",
        ".navo-held-1-2",
        ".FERRITE-HELD-1-2",
        ".Navo-Retained-ab",
    ] {
        assert!(is_retained_container(name), "{name}");
        assert!(is_reserved_name(name), "{name}");
    }
    assert!(!is_retained_container("navo-retained-ab"));
    assert!(!is_retained_container("report.navo-held-1"));
}

#[test]
fn mutation_anchors_are_reserved_but_not_retained() {
    for name in [".navo-mutations-runtime", ".ferrite-mutations-runtime"] {
        assert!(is_reserved_name(name), "{name}");
        assert!(!is_retained_container(name), "{name}");
    }
    assert!(!is_reserved_name("mutations"));
}
