use std::fs;
use std::os::windows::ffi::OsStrExt;

use super::*;
use crate::local::link::tests::fixture;
use crate::make_link;

#[test]
fn uses_explorer_shortcut_name() {
    let (_directory, source, target) = fixture();
    let created = make_link(&target, &source).unwrap();
    assert_eq!(created, target.join("report.txt - Shortcut.lnk"));
    assert!(created.is_file());
}

#[test]
fn numbers_conflicting_shortcut_names() {
    let (_directory, source, target) = fixture();
    for name in [
        "report.txt - Shortcut.lnk",
        "report.txt - Shortcut (2).lnk",
        "report.txt - Shortcut (3).lnk",
    ] {
        assert_eq!(make_link(&target, &source).unwrap(), target.join(name));
    }
}

#[test]
fn shortcut_resolves_to_source() {
    let (_directory, source, target) = fixture();
    let created = make_link(&target, &source).unwrap();
    let resolved = shortcut_target(&created);
    assert_eq!(
        fs::canonicalize(resolved).unwrap(),
        fs::canonicalize(source).unwrap()
    );
}

#[test]
fn provider_shortcut_resolves_to_source_and_is_never_overwritten() {
    let (_directory, source, target) = fixture();
    let at = target.join("report.txt - Shortcut.lnk");
    let ops = crate::LocalFs::new();
    crate::ProviderOps::make_link(&ops, &location_of(&source), &location_of(&at)).unwrap();
    let resolved = fs::canonicalize(shortcut_target(&at)).unwrap();
    assert_eq!(resolved, fs::canonicalize(&source).unwrap());
    let before = fs::read(&at).unwrap();
    let other = target.join("other.txt");
    fs::write(&other, b"other").unwrap();
    let clash = crate::ProviderOps::make_link(&ops, &location_of(&other), &location_of(&at));
    assert!(clash.is_err());
    assert_eq!(fs::read(&at).unwrap(), before);
    assert_eq!(fs::read_dir(&target).unwrap().count(), 2);
}

#[test]
fn a_shortcut_made_through_a_short_name_points_at_its_long_name_and_back() {
    let (directory, _, target) = fixture();
    let folder = directory.path().join("a folder with a long name");
    fs::create_dir(&folder).unwrap();
    let source = folder.join("quarterly report.txt");
    fs::write(&source, b"source contents").unwrap();
    let short = short_form(&source);
    let other = target.join("other.txt");
    fs::write(&other, b"other").unwrap();
    let at = target.join("quarterly report.txt - Shortcut.lnk");
    let ops = crate::LocalFs::new();
    crate::ProviderOps::make_link(&ops, &location_of(&short), &location_of(&at)).unwrap();
    for (wanted, expected) in [(&short, true), (&source, true), (&other, false)] {
        let points =
            crate::ProviderOps::link_points_at(&ops, &location_of(&at), &location_of(wanted));
        assert_eq!(points.unwrap(), expected, "{}", wanted.display());
    }
}

fn short_form(path: &Path) -> PathBuf {
    let units = wide(path).unwrap();
    let mut buffer = vec![0_u16; 32_768];
    // SAFETY: the path is NUL-terminated and both buffers outlive this synchronous call.
    let length = unsafe {
        windows::Win32::Storage::FileSystem::GetShortPathNameW(
            PCWSTR(units.as_ptr()),
            Some(&mut buffer),
        )
    };
    let length = usize::try_from(length).unwrap();
    assert!(length > 0 && length < buffer.len());
    PathBuf::from(OsString::from_wide(&buffer[..length]))
}

fn location_of(path: &Path) -> rusty_core::Location {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    rusty_core::Location::local(rusty_core::ProviderPath::from_wide(&units)).unwrap()
}

fn shortcut_target(path: &Path) -> PathBuf {
    read_target(path).unwrap()
}
