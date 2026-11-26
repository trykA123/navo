#![no_main]

use std::io::Cursor;
use std::sync::atomic::AtomicBool;

use libfuzzer_sys::fuzz_target;
use navo_automotive::{
    ElfReader, Watch, index_a2l, index_dbc, index_map, index_trace, parse_mdf, sniff, sniff_a2l,
    sniff_blf, sniff_dbc, sniff_elf, sniff_map, sniff_mdf, sniff_trace,
};

fuzz_target!(|data: &[u8]| {
    let cancel = AtomicBool::new(false);
    let mut progress = |_bytes: u64| {};
    let watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };

    if sniff_a2l(data) {
        let cursor = Cursor::new(data);
        let _ = index_a2l(cursor, watch);
    }

    let cancel = AtomicBool::new(false);
    let mut progress = |_bytes: u64| {};
    let watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    if sniff_dbc(data) {
        let cursor = Cursor::new(data);
        let _ = index_dbc(cursor, watch);
    }

    if sniff_elf(data) {
        let cancel = AtomicBool::new(false);
        let mut progress = |_bytes: u64| {};
        let mut watch = Watch {
            cancel: &cancel,
            progress: &mut progress,
        };
        let reader = ElfReader::new(Cursor::new(data));
        if let Ok(elf) = reader.parse(&mut watch) {
            let _ = reader.lines(&elf, &mut watch);
        }
    }

    if sniff_map(data).is_some() {
        let cancel = AtomicBool::new(false);
        let mut progress = |_bytes: u64| {};
        let cursor = Cursor::new(data);
        let _ = index_map(cursor, &cancel, &mut progress);
    }

    if sniff_mdf(data) {
        if let Ok(text) = std::str::from_utf8(data) {
            let _ = parse_mdf(text);
        }
    }

    if sniff_blf(data) {
        let cancel = AtomicBool::new(false);
        let mut progress = |_bytes: u64| {};
        let watch = Watch {
            cancel: &cancel,
            progress: &mut progress,
        };
        let cursor = Cursor::new(data);
        let _ = navo_automotive::index_blf(cursor, watch);
    }

    if sniff_trace(data).is_some() {
        let cancel = AtomicBool::new(false);
        let mut progress = |_bytes: u64| {};
        let watch = Watch {
            cancel: &cancel,
            progress: &mut progress,
        };
        let cursor = Cursor::new(data);
        let _ = index_trace(cursor, watch);
    }

    let _ = sniff(data);
});
