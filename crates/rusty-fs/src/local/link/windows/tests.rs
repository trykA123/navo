use std::ffi::OsString;
use std::fs;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use windows::Win32::System::Com::STGM_READ;
use windows::Win32::UI::Shell::SLGP_RAWPATH;

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

fn location_of(path: &Path) -> rusty_core::Location {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    rusty_core::Location::local(rusty_core::ProviderPath::from_wide(&units)).unwrap()
}

fn shortcut_target(path: &Path) -> PathBuf {
    let _apartment = Apartment::new().unwrap();
    let path = wide(path).unwrap();
    let mut buffer = vec![0; 32_768];
    // SAFETY: COM is initialized; the path and output buffers outlive these synchronous calls.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
        let file: IPersistFile = link.cast().unwrap();
        file.Load(PCWSTR(path.as_ptr()), STGM_READ).unwrap();
        link.GetPath(
            &mut buffer,
            std::ptr::null_mut(),
            SLGP_RAWPATH.0.cast_unsigned(),
        )
        .unwrap();
    }
    let end = buffer.iter().position(|unit| *unit == 0).unwrap();
    PathBuf::from(OsString::from_wide(&buffer[..end]))
}
