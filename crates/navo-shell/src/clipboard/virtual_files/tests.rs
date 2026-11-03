use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime};

use navo_core::NameError;

use super::descriptor::{Names, file_time, global_len, parse};
use super::hints::{inside_zip, is_rdpclip, zip_file};
use super::names::relative_path;
use super::*;

const WIDE_SIZE: usize = 72 + 520;
const ANSI_SIZE: usize = 72 + 260;

#[test]
fn names_stay_below_the_target_folder() {
    let ok = |name: &str, parts: &[&str]| {
        assert_eq!(
            relative_path(name),
            Ok(parts.iter().collect::<PathBuf>()),
            "{name} is a safe relative name"
        );
    };
    ok("report.pdf", &["report.pdf"]);
    ok("folder\\sub\\file.txt", &["folder", "sub", "file.txt"]);
    ok("..hidden", &["..hidden"]);
    ok("a..b\\c", &["a..b", "c"]);
    let table = [
        ("", BadName::Empty),
        ("\\root.txt", BadName::Absolute),
        ("\\\\server\\share\\x", BadName::Absolute),
        ("/etc/passwd", BadName::Absolute),
        ("C:\\Windows\\x.dll", BadName::Drive),
        ("c:x.txt", BadName::Drive),
        ("file.txt:secret", BadName::Stream),
        ("folder\\file:Zone.Identifier", BadName::Stream),
        ("..", BadName::Traversal),
        ("..\\up.txt", BadName::Traversal),
        ("a\\..\\..\\up.txt", BadName::Traversal),
        ("a\\\\b", BadName::EmptySegment),
        ("folder\\", BadName::EmptySegment),
        ("a\\.\\b", BadName::Segment(NameError::DotName)),
        ("a/../b", BadName::Segment(NameError::Character('/'))),
        ("CON", BadName::Segment(NameError::Reserved)),
        ("folder\\nul.txt", BadName::Segment(NameError::Reserved)),
        ("COM1.log", BadName::Segment(NameError::Reserved)),
        ("lpt9", BadName::Segment(NameError::Reserved)),
        ("trailing.", BadName::Segment(NameError::Trailing)),
        ("space \\x", BadName::Segment(NameError::Trailing)),
        ("what?.txt", BadName::Segment(NameError::Character('?'))),
        ("tab\there", BadName::Segment(NameError::Character('\t'))),
    ];
    for (name, refusal) in table {
        assert_eq!(
            relative_path(name),
            Err(refusal),
            "{name:?} must be refused"
        );
    }
    let long = "x".repeat(256);
    assert_eq!(
        relative_path(&long),
        Err(BadName::Segment(NameError::TooLong))
    );
}

struct Fd {
    flags: u32,
    attributes: u32,
    written: u64,
    size: u64,
    name: &'static str,
}

const FILE: Fd = Fd {
    flags: 0x4 | 0x20 | 0x40,
    attributes: 0x20,
    written: 133_000_000_000_000_000,
    size: 5,
    name: "a.txt",
};

fn fixed(fd: &Fd) -> Vec<u8> {
    let mut raw = vec![0u8; 72];
    raw[0..4].copy_from_slice(&fd.flags.to_le_bytes());
    raw[36..40].copy_from_slice(&fd.attributes.to_le_bytes());
    raw[56..64].copy_from_slice(&fd.written.to_le_bytes());
    raw[64..68].copy_from_slice(&u32::try_from(fd.size >> 32).unwrap().to_le_bytes());
    raw[68..72].copy_from_slice(&u32::try_from(fd.size & 0xffff_ffff).unwrap().to_le_bytes());
    raw
}

fn wide(entries: &[Fd]) -> Vec<u8> {
    let mut bytes = u32::try_from(entries.len()).unwrap().to_le_bytes().to_vec();
    for fd in entries {
        let mut raw = fixed(fd);
        let mut units: Vec<u16> = fd.name.encode_utf16().collect();
        units.resize(260, 0);
        raw.extend(units.into_iter().flat_map(u16::to_le_bytes));
        bytes.extend(raw);
    }
    bytes
}

fn ansi(entries: &[Fd]) -> Vec<u8> {
    let mut bytes = u32::try_from(entries.len()).unwrap().to_le_bytes().to_vec();
    for fd in entries {
        let mut raw = fixed(fd);
        let mut name = fd.name.as_bytes().to_vec();
        name.resize(260, 0);
        raw.extend(name);
        bytes.extend(raw);
    }
    bytes
}

#[test]
fn a_wide_descriptor_gives_names_flags_sizes_and_times() {
    let folder = Fd {
        flags: 0x4,
        attributes: 0x10,
        name: "Fotos 🦀",
        ..FILE
    };
    let big = Fd {
        size: 5 * 1024 * 1024 * 1024 + 7,
        name: "Fotos 🦀\\big.iso",
        ..FILE
    };
    let unsized_file = Fd {
        flags: 0x4,
        name: "nosize.bin",
        ..FILE
    };
    let bad = Fd {
        name: "..\\evil.exe",
        ..FILE
    };
    let mut bytes = wide(&[folder, big, unsized_file, bad]);
    bytes.extend([0; 16]);
    let entries = parse(&bytes, &Names::Wide).unwrap();
    assert_eq!(entries.len(), 4);
    assert!(entries[0].is_dir);
    assert_eq!(entries[0].path, Ok(PathBuf::from("Fotos 🦀")));
    assert_eq!(entries[0].size, None, "no FD_FILESIZE means no size");
    assert_eq!(entries[0].modified, None, "no FD_WRITESTIME means no time");
    assert!(!entries[1].is_dir);
    assert_eq!(entries[1].index, 1);
    assert_eq!(entries[1].size, Some(5 * 1024 * 1024 * 1024 + 7));
    assert_eq!(
        entries[1].path,
        Ok(["Fotos 🦀", "big.iso"].iter().collect())
    );
    assert_eq!(entries[1].modified, file_time(FILE.written));
    assert_eq!(entries[2].size, None);
    assert_eq!(entries[3].name, "..\\evil.exe");
    assert_eq!(entries[3].path, Err(BadName::Traversal));
}

#[test]
fn an_ansi_descriptor_decodes_names_with_the_given_code_page() {
    let decode = |raw: &[u8]| {
        String::from_utf8(raw.to_vec())
            .ok()
            .map(|s| s.to_uppercase())
    };
    let entries = parse(&ansi(&[FILE]), &Names::Ansi(&decode)).unwrap();
    assert_eq!(entries[0].name, "A.TXT");
    assert_eq!(entries[0].size, Some(5));
    let undecodable = |_: &[u8]| None;
    let entries = parse(&ansi(&[FILE]), &Names::Ansi(&undecodable)).unwrap();
    assert_eq!(entries[0].path, Err(BadName::NotUnicode));
    assert_eq!(ansi(&[FILE]).len(), 4 + ANSI_SIZE);
}

#[test]
fn a_short_or_lying_descriptor_is_malformed() {
    let bytes = wide(&[FILE, FILE]);
    assert_eq!(
        parse(&bytes[..4 + WIDE_SIZE], &Names::Wide),
        Err(VirtualError::Malformed)
    );
    assert_eq!(parse(&[1, 0], &Names::Wide), Err(VirtualError::Malformed));
    let huge = u32::MAX.to_le_bytes();
    assert_eq!(parse(&huge, &Names::Wide), Err(VirtualError::Malformed));
    assert_eq!(parse(&0u32.to_le_bytes(), &Names::Wide), Ok(Vec::new()));
}

#[test]
fn bad_utf16_names_are_refused_not_guessed() {
    let mut bytes = wide(&[FILE]);
    bytes[4 + 72..4 + 74].copy_from_slice(&0xd800u16.to_le_bytes());
    let entries = parse(&bytes, &Names::Wide).unwrap();
    assert_eq!(entries[0].path, Err(BadName::NotUnicode));
}

#[test]
fn file_times_convert_around_the_unix_epoch() {
    assert_eq!(file_time(0), None);
    assert_eq!(
        file_time(116_444_736_000_000_000),
        Some(SystemTime::UNIX_EPOCH)
    );
    let later = SystemTime::UNIX_EPOCH + Duration::from_millis(1_500);
    assert_eq!(file_time(116_444_736_015_000_000), Some(later));
    let earlier = SystemTime::UNIX_EPOCH - Duration::from_secs(1);
    assert_eq!(file_time(116_444_735_990_000_000), Some(earlier));
}

#[derive(Clone)]
enum Step {
    Data(&'static [u8]),
    Wait(Duration),
    Fail(&'static str),
}

#[derive(Clone)]
enum Item {
    Steps(Vec<Step>),
    Storage(Duration),
    /// A stream whose open (GetData) takes this long.
    SlowOpen(Duration, Vec<Step>),
    Refused,
}

struct Steps(Vec<Step>);

impl ItemSource for Steps {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        loop {
            if self.0.is_empty() {
                return Ok(0);
            }
            match self.0.remove(0) {
                Step::Data(bytes) => {
                    buf[..bytes.len()].copy_from_slice(bytes);
                    return Ok(bytes.len());
                }
                Step::Wait(time) => std::thread::sleep(time),
                Step::Fail(message) => return Err(message.into()),
            }
        }
    }
}

struct Fake {
    items: Vec<Item>,
    finished: Sender<bool>,
}

impl Backend for Fake {
    fn open(&mut self, index: u32) -> Result<Opened, String> {
        match self.items[usize::try_from(index).unwrap()].clone() {
            Item::Steps(steps) => Ok(Opened::Stream(Box::new(Steps(steps)))),
            Item::SlowOpen(delay, steps) => {
                std::thread::sleep(delay);
                Ok(Opened::Stream(Box::new(Steps(steps))))
            }
            Item::Storage(_) => Ok(Opened::Storage),
            Item::Refused => Err("refused".into()),
        }
    }

    fn write_storage(
        &mut self,
        index: u32,
        path: &Path,
        created: &mut dyn FnMut(),
    ) -> Result<(), String> {
        let Item::Storage(delay) = self.items[usize::try_from(index).unwrap()].clone() else {
            return Err("not a storage".into());
        };
        std::fs::write(path, b"docfile").map_err(|error| error.to_string())?;
        created();
        std::thread::sleep(delay);
        Ok(())
    }

    fn finish(&mut self, complete: bool) {
        let _ = self.finished.send(complete);
    }
}

fn fake(items: Vec<Item>) -> (VirtualClipboard, Receiver<bool>) {
    let (finished, done) = channel();
    let clipboard = VirtualClipboard::from_backend(Vec::new(), SourceKind::Unknown, move || {
        Box::new(Fake { items, finished })
    })
    .unwrap();
    (clipboard, done)
}

fn never() -> Cancel {
    Arc::new(|| false)
}

fn stream(clipboard: &VirtualClipboard, index: u32, cancel: Cancel) -> VirtualReader {
    match clipboard.open(index, cancel).unwrap() {
        VirtualContent::Stream(reader) => reader,
        VirtualContent::Storage => panic!("expected a stream"),
    }
}

fn read_error(reader: &mut VirtualReader) -> VirtualError {
    let error = reader.read_to_end(&mut Vec::new()).unwrap_err();
    VirtualError::of(&error)
        .cloned()
        .expect("a virtual-file error")
}

#[test]
fn items_stream_in_order_and_a_clean_run_finishes_complete() {
    let (clipboard, done) = fake(vec![
        Item::Steps(vec![Step::Data(b"hello "), Step::Data(b"world")]),
        Item::Steps(Vec::new()),
    ]);
    let mut bytes = Vec::new();
    stream(&clipboard, 0, never())
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"hello world");
    let mut empty = Vec::new();
    stream(&clipboard, 1, never())
        .read_to_end(&mut empty)
        .unwrap();
    assert!(empty.is_empty());
    drop(clipboard);
    assert_eq!(done.recv_timeout(Duration::from_secs(5)), Ok(true));
}

#[test]
fn nothing_read_finishes_incomplete() {
    let (clipboard, done) = fake(vec![Item::Steps(Vec::new())]);
    drop(clipboard);
    assert_eq!(done.recv_timeout(Duration::from_secs(5)), Ok(false));
}

#[test]
fn cancel_mid_item_stops_the_read_and_the_source_aborts() {
    let (clipboard, done) = fake(vec![Item::Steps(vec![
        Step::Data(b"first"),
        Step::Wait(Duration::from_secs(2)),
        Step::Data(b"never seen"),
    ])]);
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);
    let mut reader = stream(
        &clipboard,
        0,
        Arc::new(move || flag.load(Ordering::Relaxed)),
    );
    let mut first = [0u8; 5];
    reader.read_exact(&mut first).unwrap();
    cancelled.store(true, Ordering::Relaxed);
    let started = std::time::Instant::now();
    assert_eq!(read_error(&mut reader), VirtualError::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "cancel must not wait for the source"
    );
    drop(reader);
    drop(clipboard);
    assert_eq!(done.recv_timeout(Duration::from_secs(10)), Ok(false));
}

#[test]
fn a_source_that_stops_sending_fails_as_stalled() {
    let (clipboard, _done) = fake(vec![Item::Steps(vec![
        Step::Data(b"part"),
        Step::Wait(Duration::from_secs(3)),
    ])]);
    let clipboard = clipboard.with_stall(Duration::from_millis(300));
    let mut reader = stream(&clipboard, 0, never());
    let mut part = [0u8; 4];
    reader.read_exact(&mut part).unwrap();
    assert_eq!(read_error(&mut reader), VirtualError::Stalled);
    assert_eq!(
        STALL,
        Duration::from_secs(60),
        "D104 fixes the stall at 60 s"
    );
}

#[test]
fn a_source_error_mid_stream_reaches_the_reader() {
    let (clipboard, done) = fake(vec![
        Item::Steps(vec![
            Step::Data(b"part"),
            Step::Fail("the network went away"),
        ]),
        Item::Refused,
    ]);
    let mut reader = stream(&clipboard, 0, never());
    let error = read_error(&mut reader);
    assert_eq!(error, VirtualError::Source("the network went away".into()));
    assert!(matches!(clipboard.open(1, never()), Err(VirtualError::Source(m)) if m == "refused"));
    drop(reader);
    drop(clipboard);
    assert_eq!(done.recv_timeout(Duration::from_secs(5)), Ok(false));
}

#[test]
fn a_storage_item_is_saved_through_write_storage() {
    let dir = tempfile::tempdir().unwrap();
    let (clipboard, done) = fake(vec![Item::Storage(Duration::ZERO)]);
    assert!(matches!(
        clipboard.open(0, never()),
        Ok(VirtualContent::Storage)
    ));
    let path = dir.path().join("mail.msg.tmp");
    clipboard
        .write_storage(0, (&path, &mut || Ok(())), &never())
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"docfile");
    drop(clipboard);
    assert_eq!(done.recv_timeout(Duration::from_secs(5)), Ok(true));
}

#[test]
fn an_abandoned_storage_write_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let (clipboard, done) = fake(vec![Item::Storage(Duration::from_millis(500))]);
    let path = dir.path().join("mail.msg.tmp");
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        flag.store(true, Ordering::Relaxed);
    });
    let cancel: Cancel = Arc::new(move || cancelled.load(Ordering::Relaxed));
    assert_eq!(
        clipboard.write_storage(0, (&path, &mut || Ok(())), &cancel),
        Err(VirtualError::Cancelled)
    );
    canceller.join().unwrap();
    drop(clipboard);
    assert_eq!(done.recv_timeout(Duration::from_secs(5)), Ok(false));
    assert!(
        !path.exists(),
        "the reader thread removes the file nobody waits for"
    );
}

fn short_and_long(clipboard: VirtualClipboard) -> VirtualClipboard {
    clipboard
        .with_stall(Duration::from_millis(150))
        .with_long_stall(Duration::from_secs(5))
}

#[test]
fn a_slow_open_waits_for_the_long_limit_not_the_stream_stall() {
    let (clipboard, _done) = fake(vec![Item::SlowOpen(
        Duration::from_millis(500),
        vec![Step::Data(b"whole file")],
    )]);
    let clipboard = short_and_long(clipboard);
    let mut bytes = Vec::new();
    stream(&clipboard, 0, never())
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"whole file");
}

#[test]
fn a_stream_that_gives_no_bytes_after_it_opened_fails_at_the_short_limit() {
    let (clipboard, _done) = fake(vec![Item::Steps(vec![Step::Wait(Duration::from_secs(3))])]);
    let clipboard = short_and_long(clipboard);
    let started = std::time::Instant::now();
    assert!(matches!(
        clipboard.open(0, never()),
        Err(VirtualError::Stalled)
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(LONG_STALL, Duration::from_secs(600));
}

#[test]
fn a_slow_storage_write_waits_for_the_long_limit() {
    let dir = tempfile::tempdir().unwrap();
    let (clipboard, _done) = fake(vec![Item::Storage(Duration::from_millis(500))]);
    let clipboard = short_and_long(clipboard);
    let path = dir.path().join("mail.msg.tmp");
    clipboard
        .write_storage(0, (&path, &mut || Ok(())), &never())
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"docfile");
}

#[test]
fn the_created_hook_runs_when_the_file_exists_and_before_the_copy_ends() {
    let dir = tempfile::tempdir().unwrap();
    let (clipboard, _done) = fake(vec![Item::Storage(Duration::from_millis(600))]);
    let path = dir.path().join("mail.msg.tmp");
    let mut seen = Vec::new();
    let started = std::time::Instant::now();
    clipboard
        .write_storage(
            0,
            (&path, &mut || {
                seen.push((path.exists(), started.elapsed()));
                Ok(())
            }),
            &never(),
        )
        .unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0, "the file exists when the hook runs");
    assert!(seen[0].1 < Duration::from_millis(400), "{:?}", seen[0].1);
}

#[test]
fn a_failing_created_hook_stops_the_wait_and_the_file_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (clipboard, done) = fake(vec![Item::Storage(Duration::from_millis(300))]);
    let path = dir.path().join("mail.msg.tmp");
    let result = clipboard.write_storage(0, (&path, &mut || Err("journal full".into())), &never());
    assert_eq!(result, Err(VirtualError::Source("journal full".into())));
    drop(clipboard);
    assert_eq!(done.recv_timeout(Duration::from_secs(5)), Ok(false));
    assert!(!path.exists(), "the reader thread removes the file");
}

#[test]
fn an_hglobal_needs_a_stated_size_and_uses_exactly_that_size() {
    assert_eq!(global_len(Some(5), 4096), Ok(5));
    assert_eq!(global_len(Some(4096), 4096), Ok(4096));
    assert_eq!(global_len(Some(0), 0), Ok(0), "an empty file is valid");
    assert_eq!(global_len(Some(0), 4096), Ok(0));
    assert!(global_len(Some(5000), 4096).is_err(), "smaller than stated");
    assert!(global_len(Some(u64::MAX), 4096).is_err());
    let error = global_len(None, 4096).unwrap_err();
    assert_eq!(error, "the program did not say how large this file is");
}

#[test]
fn only_the_system_rdpclip_counts_as_a_remote_session() {
    let root = "C:\\Windows";
    assert!(is_rdpclip("C:\\Windows\\System32\\rdpclip.exe", root));
    assert!(is_rdpclip(
        "c:\\windows\\system32\\RDPCLIP.EXE",
        "C:\\WINDOWS\\"
    ));
    assert!(!is_rdpclip("C:\\Users\\x\\Downloads\\rdpclip.exe", root));
    assert!(!is_rdpclip("C:\\Windows\\System32\\rdpclip.exe.evil", root));
    assert!(!is_rdpclip(
        "C:\\Windows\\Temp\\System32\\rdpclip.exe",
        root
    ));
    assert!(!is_rdpclip("rdpclip", root), "a bare name is not a path");
    assert!(!is_rdpclip("C:\\Windows\\System32\\rdpclip.exe", ""));
}

#[test]
fn the_zip_file_comes_from_the_folder_name() {
    let zip = |name: &str| zip_file(name).map(|path| path.display().to_string());
    assert_eq!(
        zip("C:\\Users\\x\\a.zip").as_deref(),
        Some("C:\\Users\\x\\a.zip")
    );
    assert_eq!(
        zip("C:\\d\\A.ZIP\\inner\\deep").as_deref(),
        Some("C:\\d\\A.ZIP")
    );
    assert_eq!(
        zip("\\\\server\\share\\a.zip").as_deref(),
        Some("\\\\server\\share\\a.zip")
    );
    assert_eq!(zip("C:\\d\\a.zipx"), None);
    assert_eq!(zip("::{20D04FE0}\\a.zip"), None, "not a file path");
    assert_eq!(zip("a.zip"), None, "not absolute");
    assert!(inside_zip("::{20D04FE0}\\a.zip"), "still an archive");
    assert!(!inside_zip("C:\\d\\a.zipx"));
}
