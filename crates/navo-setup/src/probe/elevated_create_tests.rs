// Windows tests for E5's create relative to held handles; unelevated, in temp folders at the
// system drive root, with a user-owned descriptor because only an administrator can set `O:BA`.

#![cfg(windows)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
};

use super::{create_verified, plain_name};
use crate::elevate::{HelperArgs, HelperExit};
use crate::payload::platform::security;
use crate::placement::FileId;
use crate::probe::chain::resolve_path;
use crate::probe::chain::tests::mount_point_in_place;
use crate::probe::helper_checks::helper_chain;

/// The parts of a descriptor the tests compare.
const PARTS: u32 = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;

/// The IW25 ACEs with the test user as owner, since an unelevated test cannot set `O:BA`.
fn user_owned_sddl(me: &str) -> String {
    format!("O:{me}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;{me})")
}

/// A temp folder at the system drive's root: `%TEMP%` sits inside a user profile, which refuses.
fn outside_profiles() -> tempfile::TempDir {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_owned());
    tempfile::Builder::new()
        .prefix(".navo-e5-")
        .tempdir_in(format!("{drive}\\"))
        .expect("temp folder at the system drive root")
}

/// The identity of an existing folder, read through its handle.
fn id_of(path: &Path) -> FileId {
    resolve_path(path)
        .expect("chain resolves")
        .existing
        .last()
        .expect("path exists")
        .id
}

/// Helper arguments for `root` under `parent`, for this process's own user.
fn args(root: PathBuf, parent: FileId) -> HelperArgs {
    HelperArgs {
        root,
        user_sid: security::user_sid().expect("token user SID"),
        parent,
    }
}

/// Renders a descriptor's owner and DACL as SDDL, without the auto-inherited mark, which NTFS
/// may add to a created folder's protected DACL.
fn render(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<String> {
    let mut text = std::ptr::null_mut();
    let mut length = 0;
    // SAFETY: the descriptor is live and the text and length outputs are writable.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            1,
            PARTS,
            &mut text,
            &mut length,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let _text = security::Local(text.cast());
    let length = usize::try_from(length).map_err(io::Error::other)?;
    // SAFETY: a successful conversion returns a live string of `length` UTF-16 units.
    let units = unsafe { std::slice::from_raw_parts(text, length) };
    let rendered = String::from_utf16_lossy(units);
    Ok(rendered
        .trim_end_matches('\0')
        .replacen("D:PAI(", "D:P(", 1))
}

/// Reads a folder's owner and DACL back as SDDL text.
fn folder_sddl(path: &Path) -> io::Result<String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: the wide path is NUL-terminated and every optional output is null or writable.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            PARTS,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status.cast_signed()));
    }
    let _allocation = security::Local(descriptor);
    render(descriptor)
}

/// `sddl` rendered by the same converter as `folder_sddl`, so only content can differ.
fn expected_sddl(sddl: &str) -> String {
    let descriptor = security::descriptor(sddl).expect("test SDDL converts");
    render(descriptor.0).expect("test descriptor renders")
}

/// Whether `path` holds no entry; a folder that cannot be listed fails the test.
fn is_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .expect("folder lists")
        .next()
        .is_none()
}

/// The junction-swap test (IW42): the holds stop a middle link from being renamed away, and a
/// junction set in place on the held nearest folder after `helper_chain` is `Changed` before
/// anything is created through it.
#[test]
fn a_junction_set_in_after_helper_chain_is_changed_and_creates_nothing() {
    let temp = outside_profiles();
    let middle = temp.path().join("middle");
    let nearest = middle.join("nearest");
    let elsewhere = temp.path().join("elsewhere");
    std::fs::create_dir_all(&nearest).expect("existing folders");
    std::fs::create_dir(&elsewhere).expect("junction target");
    let me = security::user_sid().expect("token user SID");
    let args = args(nearest.join("Tools").join("Navo"), id_of(&nearest));
    let held = helper_chain(&args).expect("the chain passes the helper's checks");

    let renamed = std::fs::rename(&middle, temp.path().join("moved"));
    let sharing_violation = i32::try_from(ERROR_SHARING_VIOLATION).expect("error code fits i32");
    assert_eq!(
        renamed.map_err(|error| error.raw_os_error()),
        Err(Some(sharing_violation)),
        "a held middle link cannot be swapped"
    );
    mount_point_in_place(&nearest, &elsewhere).expect("a hold does not stop an in-place junction");

    let result = create_verified(&args, &held, &user_owned_sddl(&me));
    assert_eq!(result, Err(HelperExit::Changed));
    assert!(
        is_empty(&elsewhere),
        "nothing was created through the junction"
    );
}

#[test]
fn two_new_folders_carry_the_given_descriptor() {
    let temp = outside_profiles();
    let me = security::user_sid().expect("token user SID");
    let sddl = user_owned_sddl(&me);
    let tools = temp.path().join("Tools");
    let args = args(tools.join("Navo"), id_of(temp.path()));
    let held = helper_chain(&args).expect("the chain passes the helper's checks");
    assert_eq!(create_verified(&args, &held, &sddl), Ok(()));
    drop(held);
    for folder in [&tools, &tools.join("Navo")] {
        let found = folder_sddl(folder).expect("descriptor reads back");
        assert_eq!(found, expected_sddl(&sddl), "{}", folder.display());
    }
}

#[test]
fn a_wrong_parent_id_creates_nothing() {
    let temp = outside_profiles();
    let other = temp.path().join("other");
    std::fs::create_dir(&other).expect("sibling folder");
    let me = security::user_sid().expect("token user SID");
    let root = temp.path().join("Tools").join("Navo");
    let held = helper_chain(&args(root.clone(), id_of(temp.path()))).expect("checks pass");
    let wrong = args(root, id_of(&other));
    let result = create_verified(&wrong, &held, &user_owned_sddl(&me));
    assert_eq!(result, Err(HelperExit::Changed));
    assert!(!temp.path().join("Tools").exists(), "nothing was created");
}

#[test]
fn a_name_that_appears_after_the_checks_is_exists_and_stays() {
    let temp = outside_profiles();
    let me = security::user_sid().expect("token user SID");
    let tools = temp.path().join("Tools");
    let args = args(tools.join("Navo"), id_of(temp.path()));
    let held = helper_chain(&args).expect("the chain passes the helper's checks");
    std::fs::create_dir(&tools).expect("a folder made by someone else");
    std::fs::write(tools.join("theirs.txt"), b"kept").expect("their file");
    let result = create_verified(&args, &held, &user_owned_sddl(&me));
    assert_eq!(result, Err(HelperExit::Exists));
    assert!(
        tools.join("theirs.txt").is_file(),
        "nothing it did not create is removed"
    );
    assert!(!tools.join("Navo").exists(), "nothing was created below it");
}

/// A root whose existing part no longer resolves to the held folders fails step 3, and the
/// rollback deletes what this call created, newest first.
#[test]
fn a_root_that_resolves_elsewhere_is_changed_and_rolled_back() {
    let temp = outside_profiles();
    let elsewhere = outside_profiles();
    let me = security::user_sid().expect("token user SID");
    let parent = id_of(temp.path());
    let held = helper_chain(&args(temp.path().join("Tools").join("Navo"), parent))
        .expect("the chain passes the helper's checks");
    let moved = args(elsewhere.path().join("Tools").join("Navo"), parent);
    let result = create_verified(&moved, &held, &user_owned_sddl(&me));
    assert_eq!(result, Err(HelperExit::Changed));
    drop(held);
    assert!(is_empty(temp.path()), "the created folders were removed");
}

/// Missing names that are not the root's last names are `Changed` before any create: a folder
/// made by someone else at the first name would otherwise give `Exists`.
#[test]
fn missing_names_that_are_not_the_root_tail_are_changed_before_any_create() {
    let temp = outside_profiles();
    let me = security::user_sid().expect("token user SID");
    let parent = id_of(temp.path());
    let held = helper_chain(&args(temp.path().join("Tools").join("Navo"), parent))
        .expect("the chain passes the helper's checks");
    std::fs::create_dir(temp.path().join("Tools")).expect("a folder made by someone else");
    let other = args(temp.path().join("Other").join("Navo"), parent);
    let result = create_verified(&other, &held, &user_owned_sddl(&me));
    assert_eq!(result, Err(HelperExit::Changed));
    assert!(is_empty(&temp.path().join("Tools")), "nothing was created");
}

#[test]
fn separators_streams_and_dots_are_not_plain_names() {
    for name in [
        "a\\b", "a/b", "a:b", "a:", ".", "..", "", "a\0b", "x..", "Navo ",
    ] {
        assert!(!plain_name(OsStr::new(name)), "{name:?}");
    }
    for name in ["Navo", "Navo.Tools", "..x", " Navo"] {
        assert!(plain_name(OsStr::new(name)), "{name:?}");
    }
}

#[test]
fn device_names_are_not_plain_names() {
    for name in [
        "CON",
        "prn",
        "Aux",
        "NUL",
        "nul.txt",
        "COM1",
        "com9.log",
        "LPT1",
        "lpt9",
        "COM¹",
        "LPT³.txt",
        "CON .x",
    ] {
        assert!(!plain_name(OsStr::new(name)), "{name:?}");
    }
    for name in ["CONSOLE", "COM10", "LPT", "Navo.CON"] {
        assert!(plain_name(OsStr::new(name)), "{name:?}");
    }
}
