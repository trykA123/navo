// Windows tests for E8's adopt: held identity, descriptor and empty-root checks, never a delete.

#![cfg(windows)]

use std::mem::size_of;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

use super::super::{Pin, PinError, wide_z};
use super::{Accept, adopt_checked};
use crate::elevate::{AceView, admin_descriptor_ok};
use crate::payload::platform::security;
use crate::placement::Chain;
use crate::probe::chain::resolve_path;

/// The IW25 ACEs with the test user as owner, since an unelevated test cannot set `O:BA` (E5).
fn user_owned_sddl(me: &str) -> String {
    format!("O:{me}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;{me})")
}

/// Accepts exactly `user_owned_sddl(me)`: owner `me`, then the IW25 ACE rule.
fn accept_user_owned(me: &str) -> impl Fn(&str, bool, &[AceView]) -> bool + '_ {
    move |owner, protected, aces| {
        owner == me && admin_descriptor_ok("S-1-5-32-544", protected, aces, me)
    }
}

/// Creates one folder with the given SDDL, as the elevated helper would.
fn make(path: &Path, sddl: &str) {
    let descriptor = security::descriptor(sddl).expect("descriptor");
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).expect("size"),
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let wide = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and the descriptor stays live for the call.
    let made = unsafe { CreateDirectoryW(wide.as_ptr(), std::ptr::from_ref(&attributes)) };
    assert_ne!(made, 0, "create {}", path.display());
}

/// A temp base, the `before` chain for `base\Navo\app`, and that root path.
fn setup() -> (tempfile::TempDir, Chain, PathBuf) {
    let temp = tempfile::tempdir().expect("temp folder");
    let root = temp.path().join("Navo").join("app");
    let before = resolve_path(&root).expect("before resolves");
    (temp, before, root)
}

/// Makes `Navo` and `app` with `sddl` and resolves the `after` chain.
fn made_chain(root: &Path, sddl: &str) -> Chain {
    make(root.parent().expect("parent"), sddl);
    make(root, sddl);
    resolve_path(root).expect("after resolves")
}

fn adopt_err(before: &Chain, after: &Chain, accept: &Accept<'_>) -> PinError {
    adopt_checked(before, after, accept)
        .err()
        .expect("adopt must refuse")
}

#[test]
fn adopted_folders_are_held_and_remove_created_removes_them() {
    let me = security::user_sid().expect("user sid");
    let (temp, before, root) = setup();
    let after = made_chain(&root, &user_owned_sddl(&me));
    let pin = adopt_checked(&before, &after, &accept_user_owned(&me)).expect("adopt");
    assert_eq!(pin.root(), Some(root.as_path()));
    assert!(pin.unchanged());
    assert!(std::fs::rename(temp.path().join("Navo"), temp.path().join("moved")).is_err());
    let removal = pin.remove_created();
    assert_eq!(removal.removed.len(), 2, "{removal:?}");
    assert!(!temp.path().join("Navo").exists());
}

#[test]
fn pin_adopt_refuses_a_user_owned_folder() {
    let me = security::user_sid().expect("user sid");
    let (_temp, before, root) = setup();
    let after = made_chain(&root, &user_owned_sddl(&me));
    let error = Pin::adopt(&before, &after, &me).err().expect("refused");
    assert!(matches!(error, PinError::Descriptor), "{error:?}");
    assert!(root.exists(), "adopt never deletes");
}

#[test]
fn a_protected_dacl_without_the_user_ace_is_descriptor() {
    let me = security::user_sid().expect("user sid");
    let (_temp, before, root) = setup();
    // Authenticated Users stands in for the user, so the test can still open the folder.
    let sddl = format!("O:{me}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;AU)");
    let after = made_chain(&root, &sddl);
    let error = adopt_err(&before, &after, &accept_user_owned(&me));
    assert!(matches!(error, PinError::Descriptor), "{error:?}");
}

#[test]
fn a_renamed_and_replaced_root_is_identity() {
    let me = security::user_sid().expect("user sid");
    let sddl = user_owned_sddl(&me);
    let (_temp, before, root) = setup();
    let after = made_chain(&root, &sddl);
    std::fs::rename(&root, root.with_file_name("old")).expect("rename");
    make(&root, &sddl);
    let error = adopt_err(&before, &after, &accept_user_owned(&me));
    assert!(matches!(error, PinError::Identity), "{error:?}");
    assert!(
        root.with_file_name("old").exists() && root.exists(),
        "adopt never deletes"
    );
}

#[test]
fn a_root_with_an_entry_is_root_exists() {
    let me = security::user_sid().expect("user sid");
    let (_temp, before, root) = setup();
    let after = made_chain(&root, &user_owned_sddl(&me));
    std::fs::write(root.join("planted.dll"), b"x").expect("plant");
    let error = adopt_err(&before, &after, &accept_user_owned(&me));
    assert!(matches!(error, PinError::RootExists), "{error:?}");
    assert!(root.join("planted.dll").exists(), "adopt never deletes");
}

#[test]
fn an_unfinished_chain_is_identity() {
    let me = security::user_sid().expect("user sid");
    let (_temp, before, root) = setup();
    make(root.parent().expect("parent"), &user_owned_sddl(&me));
    let after = resolve_path(&root).expect("after resolves");
    let error = adopt_err(&before, &after, &accept_user_owned(&me));
    assert!(matches!(error, PinError::Identity), "{error:?}");
}
