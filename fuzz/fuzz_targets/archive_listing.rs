#![no_main]

use std::io::Write;
use std::sync::atomic::AtomicBool;

use libfuzzer_sys::fuzz_target;
use navo_fs::archive::list_archive;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let Ok(mut tmp) = tempfile::NamedTempFile::new() else {
        return;
    };
    if tmp.write_all(data).is_err() {
        return;
    }
    tmp.flush().ok();
    let cancel = AtomicBool::new(false);
    let _ = list_archive(tmp.path(), &cancel);
});
