#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    reason = "test helpers fail the test on a broken fixture"
)]

use std::path::PathBuf;

use rusty_shell::{DragError, data_object, decode_drop_files};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Ole::{CF_HDROP, OleInitialize, OleUninitialize, ReleaseStgMedium};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

struct Ole;

impl Ole {
    fn init() -> Self {
        // SAFETY: paired with OleUninitialize in Drop on this test thread.
        unsafe { OleInitialize(None) }.unwrap();
        Self
    }
}

impl Drop for Ole {
    fn drop(&mut self) {
        // SAFETY: OleInitialize succeeded on this thread.
        unsafe { OleUninitialize() };
    }
}

fn format(cf: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: cf,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
        ..FORMATETC::default()
    }
}

fn hdrop_paths(data: &IDataObject) -> Vec<PathBuf> {
    // SAFETY: the medium is an HGLOBAL, locked only while it is read and released once.
    unsafe {
        let mut medium = data.GetData(&format(CF_HDROP.0)).unwrap();
        let global = medium.u.hGlobal.0;
        let size = GlobalSize(global);
        let bytes = GlobalLock(global).cast::<u8>();
        assert!(!bytes.is_null());
        let decoded = decode_drop_files(std::slice::from_raw_parts(bytes, size));
        GlobalUnlock(global);
        ReleaseStgMedium(&mut medium);
        decoded.unwrap()
    }
}

fn two_files() -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().unwrap();
    let files = ["first.txt", "second.txt"].map(|name| {
        let path = dir.path().join(name);
        std::fs::write(&path, name).unwrap();
        path
    });
    (dir, files.to_vec())
}

fn canonical(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|path| std::fs::canonicalize(path).unwrap())
        .collect()
}

#[test]
fn drag_out_data_object_renders_the_same_paths_as_hdrop() {
    let _ole = Ole::init();
    let (_dir, files) = two_files();
    let data = data_object(&files).unwrap();
    assert_eq!(canonical(&hdrop_paths(&data)), canonical(&files));
}

#[test]
fn drag_out_data_object_offers_a_shell_id_list() {
    let _ole = Ole::init();
    let (_dir, files) = two_files();
    let data = data_object(&files).unwrap();
    // SAFETY: a static wide string in; the format query only reads the FORMATETC.
    let offered = unsafe {
        let id_list = RegisterClipboardFormatW(windows::core::w!("Shell IDList Array"));
        assert_ne!(id_list, 0);
        data.QueryGetData(&format(id_list as u16))
    };
    assert!(offered.is_ok(), "{offered:?}");
}

#[test]
fn drag_out_refuses_files_from_two_folders() {
    let _ole = Ole::init();
    let (dir, mut files) = two_files();
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let nested = sub.join("third.txt");
    std::fs::write(&nested, "third").unwrap();
    files.push(nested);
    assert!(matches!(data_object(&files), Err(DragError::MixedFolders)));
}
