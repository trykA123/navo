use super::*;

/// Runs the rule on two Windows path texts; works the same on Linux.
fn allowed(root: &str, packages: &str) -> bool {
    updates_allowed(Path::new(root), Path::new(packages))
}

#[test]
fn a_packages_folder_under_localappdata_is_not_allowed() {
    assert!(!allowed(
        r"C:\Program Files\Navo App",
        r"C:\Users\admin\AppData\Local\NavoApp\packages"
    ));
}

#[test]
fn a_packages_folder_inside_the_root_is_allowed_whatever_the_case() {
    assert!(allowed(
        r"C:\Program Files\Navo App",
        r"c:\program files\navo app\packages"
    ));
}

#[test]
fn the_long_path_prefix_is_stripped_from_both() {
    assert!(allowed(r"\\?\C:\Navo", r"C:\Navo\packages"));
    assert!(allowed(r"C:\Navo", r"\\?\c:\navo\packages"));
    assert!(!allowed(r"\\?\C:\Navo", r"\\?\D:\Navo\packages"));
}

#[test]
fn components_compare_whole_not_as_text_prefixes() {
    assert!(!allowed(r"C:\Navo", r"C:\Navo App\packages"));
    assert!(allowed(r"C:\Navo\", r"C:/Navo/packages"));
}
