use std::fs;

use rusty_core::Location;
use rusty_core::encoding::TextEncoding;
use rusty_ui::window_view::{Save, SaveError};

use super::{line_endings, mostly_crlf, read, write};

mod encodings;

fn at(path: &std::path::Path) -> Location {
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

fn save(text: &str, crlf: bool, expected: Option<rusty_ui::window_view::FileStamp>) -> Save<'_> {
    Save {
        text,
        crlf,
        expected,
        encoding: TextEncoding::default(),
    }
}

#[test]
fn text_reads_back_and_saves_without_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.md");
    fs::write(&file, "# hi\n").unwrap();
    fs::write(dir.path().join("notes.md.bak"), "mine").unwrap();
    let text = read(&at(&file)).unwrap();
    assert_eq!(
        (text.text.as_str(), text.read_only, text.crlf),
        ("# hi\n", false, false)
    );
    write(&at(&file), &save("# bye\n", text.crlf, text.stamp)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "# bye\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("notes.md.bak")).unwrap(),
        "mine"
    );
}

#[test]
fn crlf_files_stay_crlf_even_for_new_lines() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "one\r\ntwo\r\n").unwrap();
    let text = read(&at(&file)).unwrap();
    assert!(text.crlf);
    let edited = format!("{}three\n", text.text);
    write(&at(&file), &save(&edited, true, text.stamp)).unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"one\r\ntwo\r\nthree\r\n");
}

#[test]
fn pasted_crlf_in_an_lf_file_becomes_lf() {
    assert_eq!(line_endings("a\r\nb\nc", false), "a\nb\nc");
    assert_eq!(line_endings("a\r\nb\nc", true), "a\r\nb\r\nc");
}

#[test]
fn a_file_changed_on_disk_is_not_overwritten_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "old").unwrap();
    let text = read(&at(&file)).unwrap();
    fs::write(&file, "changed elsewhere, longer").unwrap();
    let refused = write(&at(&file), &save("mine", false, text.stamp));
    assert_eq!(refused, Err(SaveError::Changed));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "changed elsewhere, longer"
    );
    write(&at(&file), &save("mine", false, None)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "mine");
}

#[test]
fn binary_and_huge_files_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("a.bin");
    fs::write(&binary, [1_u8, 0, 2]).unwrap();
    assert!(read(&at(&binary)).unwrap_err().contains("binary"));
    let huge = dir.path().join("huge.log");
    let file = fs::File::create(&huge).unwrap();
    file.set_len(17 * 1024 * 1024).unwrap();
    assert!(read(&at(&huge)).unwrap_err().contains("Too large"));
}

#[test]
fn invalid_utf8_opens_read_only_with_or_without_a_bom() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("broken.txt");
    fs::write(&file, [0xEF, 0xBB, 0xBF, b'c', 0xE9, b'\n']).unwrap();
    assert!(read(&at(&file)).unwrap().read_only);
    let bare = dir.path().join("bare.txt");
    let bytes = [
        "café crème brûlée ".as_bytes(),
        &[0xFF],
        " déjà vu\n".as_bytes(),
    ]
    .concat();
    fs::write(&bare, bytes).unwrap();
    let opened = read(&at(&bare)).unwrap();
    assert_eq!(opened.text, "café crème brûlée \u{FFFD} déjà vu\n");
    assert!(opened.read_only);
}

#[test]
fn a_missing_or_remote_file_explains_itself() {
    assert!(read(&Location::new("/no/such/file.txt").unwrap()).is_err());
    assert!(read(&Location::new("fake://home/a.txt").unwrap()).is_err());
}

#[test]
fn the_majority_line_ending_wins() {
    assert!(mostly_crlf(b"a\r\nb\r\nc\n"));
    assert!(!mostly_crlf(b"a\nb\nc\r\n"));
    assert!(!mostly_crlf(b"no newline"));
}

#[cfg(unix)]
#[test]
fn saving_keeps_the_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".env");
    fs::write(&file, "KEY=1\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let text = read(&at(&file)).unwrap();
    write(&at(&file), &save("KEY=2\n", false, text.stamp)).unwrap();
    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

fn exact(path: &std::path::Path) -> std::path::PathBuf {
    if cfg!(windows) {
        std::path::PathBuf::from(format!(r"\\?\{}", path.display()))
    } else {
        path.to_owned()
    }
}

#[test]
fn a_trailing_dot_name_is_read_and_saved_as_itself() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(exact(&dir.path().join("report")), "plain\n").unwrap();
    fs::write(exact(&dir.path().join("report.")), "dotted\n").unwrap();
    let dotted = at(&dir.path().join("report."));
    assert_eq!(read(&dotted).unwrap().text, "dotted\n");
    write(&dotted, &save("changed\n", false, None)).unwrap();
    assert_eq!(
        fs::read_to_string(exact(&dir.path().join("report"))).unwrap(),
        "plain\n"
    );
    assert_eq!(
        fs::read_to_string(exact(&dir.path().join("report."))).unwrap(),
        "changed\n"
    );
}

#[test]
fn the_hooks_list_a_zip_and_read_one_entry() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bundle.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&file).unwrap());
    writer
        .start_file("inner/notes.md", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"# inside\n").unwrap();
    writer.finish().unwrap();
    let hooks = super::hooks();
    let keep = std::sync::atomic::AtomicBool::new(false);
    let listing = (hooks.archive)(&at(&file), &keep).unwrap();
    assert_eq!(listing.entries[0].name, "inner/notes.md");
    assert!(!listing.truncated);
    let entry = (hooks.archive_entry)(&at(&file), 0, &keep);
    assert_eq!(entry.unwrap(), b"# inside\n");
    assert!((hooks.archive_entry)(&at(&file), 1, &keep).is_err());
}

#[test]
fn whole_file_bytes_stop_at_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("face.ttf");
    fs::write(&file, [7u8; 10]).unwrap();
    let hooks = super::hooks();
    assert_eq!((hooks.bytes)(&at(&file), 10).unwrap(), [7u8; 10]);
    assert!(
        (hooks.bytes)(&at(&file), 9)
            .unwrap_err()
            .contains("too large")
    );
}
