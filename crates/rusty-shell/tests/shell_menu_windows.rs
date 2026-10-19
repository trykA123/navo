#![cfg(windows)]
#![allow(clippy::print_stderr, reason = "timings are printed for the CI log")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on a broken fixture"
)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusty_core::shell_menu::{ItemId, ItemKind, MenuItem, ShellMenu, Submenu};
use rusty_shell::{
    FILL_BUDGET, InvokeOptions, MenuError, MenuSession, ScreenPoint, ShellMenuService, WindowHandle,
};
use tempfile::TempDir;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, IsClipboardFormatAvailable, OpenClipboard,
};
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::System::Threading::{
    GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources, GetProcessHandleCount,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_KEYDOWN,
};
use windows::core::w;

const ANSWER: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(10);

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn file(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, b"ferrite shell menu test").unwrap();
        path
    }

    fn zip(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        writer
            .start_file("inner.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"inside").unwrap();
        writer.finish().unwrap();
        path
    }

    fn folder(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::create_dir(&path).unwrap();
        path
    }
}

fn open(service: &ShellMenuService, paths: Vec<PathBuf>) -> MenuSession {
    let started = Instant::now();
    let session = service
        .open(paths)
        .wait_timeout(ANSWER)
        .expect("the shell answers")
        .expect("the menu opens");
    eprintln!(
        "open: {} items in {} ms",
        session.menu().items.len(),
        started.elapsed().as_millis()
    );
    session
}

fn all_items(items: &[MenuItem]) -> Vec<&MenuItem> {
    items
        .iter()
        .flat_map(|item| {
            let children = item.children().map(all_items).unwrap_or_default();
            std::iter::once(item).chain(children)
        })
        .collect()
}

fn verb_of(item: &MenuItem) -> Option<&str> {
    match &item.id {
        ItemId::Shell { verb, .. } => verb.as_deref(),
        ItemId::Ferrite(_) | ItemId::Action(_) => None,
    }
}

fn find_verb<'a>(menu: &'a ShellMenu, verb: &str) -> Option<&'a MenuItem> {
    all_items(&menu.items)
        .into_iter()
        .find(|item| verb_of(item).is_some_and(|v| v.eq_ignore_ascii_case(verb)))
}

fn assert_core_verbs(paths: Vec<PathBuf>) {
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, paths);
    for verb in ["open", "copy", "delete"] {
        assert!(
            find_verb(session.menu(), verb).is_some(),
            "no `{verb}` in {:?}",
            session.menu()
        );
    }
}

#[test]
fn a_text_file_menu_has_open_copy_and_delete() {
    let fixture = Fixture::new();
    assert_core_verbs(vec![fixture.file("notes.txt")]);
}

#[test]
fn a_zip_menu_has_open_copy_and_delete() {
    let fixture = Fixture::new();
    assert_core_verbs(vec![fixture.zip("bundle.zip")]);
}

#[test]
fn a_folder_menu_has_open_copy_and_delete() {
    let fixture = Fixture::new();
    assert_core_verbs(vec![fixture.folder("docs")]);
}

#[test]
fn a_three_file_menu_has_open_copy_and_delete() {
    let fixture = Fixture::new();
    let paths = ["one.txt", "two.txt", "three.txt"].map(|name| fixture.file(name));
    assert_core_verbs(paths.to_vec());
}

#[test]
fn some_items_carry_rgba_icons() {
    let fixture = Fixture::new();
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, vec![fixture.file("notes.txt")]);
    let icons: Vec<_> = all_items(&session.menu().items)
        .into_iter()
        .filter_map(|item| item.icon.as_ref())
        .collect();
    eprintln!("icons: {}", icons.len());
    assert!(!icons.is_empty());
    for icon in icons {
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }
}

fn send_to_position(session: &MenuSession) -> usize {
    let position = session
        .menu()
        .items
        .iter()
        .position(|item| item.text.eq_ignore_ascii_case("Send to") && item.is_submenu())
        .expect("a Send to submenu");
    eprintln!(
        "send to before fill: {:?}",
        session.menu().items[position].kind
    );
    position
}

#[test]
fn send_to_answers_within_the_budget() {
    let fixture = Fixture::new();
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, vec![fixture.file("notes.txt")]);
    let position = send_to_position(&session);
    let started = Instant::now();
    let submenu = session
        .fill(&[position])
        .wait_timeout(ANSWER)
        .unwrap()
        .unwrap();
    let elapsed = started.elapsed();
    let kind = match &submenu {
        Submenu::Filled(children) => format!("filled with {}", children.len()),
        other => format!("{other:?}"),
    };
    eprintln!("send to first answer: {kind} in {} ms", elapsed.as_millis());
    let reopened = Instant::now();
    drop(open(&service, vec![fixture.file("again.txt")]));
    let round_trip = reopened.elapsed();
    eprintln!("open after the fill: {} ms", round_trip.as_millis());
    assert!(
        matches!(submenu, Submenu::Filled(_) | Submenu::Loading),
        "{submenu:?}"
    );
    assert!(
        elapsed < FILL_BUDGET * 4,
        "the first answer took {elapsed:?}"
    );
    assert!(
        round_trip < Duration::from_secs(10),
        "the next open took {round_trip:?}"
    );
}

#[test]
fn a_loading_submenu_fills_later() {
    let fixture = Fixture::new();
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, vec![fixture.file("notes.txt")]);
    let position = send_to_position(&session);
    let started = Instant::now();
    let deadline = started + Duration::from_secs(15);
    let children = loop {
        match session.fill(&[position]).wait_timeout(ANSWER).unwrap() {
            Ok(Submenu::Filled(children)) => break children,
            Ok(Submenu::Loading) if Instant::now() < deadline => std::thread::sleep(POLL),
            other => panic!("Send to did not fill: {other:?}"),
        }
    };
    let names: Vec<_> = children.iter().map(|child| child.text.as_str()).collect();
    eprintln!(
        "send to: {} items in {} ms: {names:?}",
        children.len(),
        started.elapsed().as_millis()
    );
    assert!(children.iter().any(|child| !child.text.is_empty()));
}

#[test]
fn fill_on_a_command_gives_no_submenu() {
    let fixture = Fixture::new();
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, vec![fixture.file("notes.txt")]);
    let position = session
        .menu()
        .items
        .iter()
        .position(|item| item.kind == ItemKind::Command)
        .unwrap();
    let result = session.fill(&[position]).wait_timeout(ANSWER).unwrap();
    assert_eq!(result, Err(MenuError::NoSubmenu(vec![position])));
}

fn clipboard_has_file_drop() -> bool {
    // SAFETY: the clipboard is opened by this thread and closed before returning.
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let found = IsClipboardFormatAvailable(u32::from(CF_HDROP.0)).is_ok();
        let _ = CloseClipboard();
        found
    }
}

#[test]
fn invoking_copy_puts_a_file_drop_on_the_clipboard() {
    let fixture = Fixture::new();
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, vec![fixture.file("notes.txt")]);
    // SAFETY: the clipboard is opened by this thread and closed right away.
    unsafe {
        OpenClipboard(None).unwrap();
        EmptyClipboard().unwrap();
        CloseClipboard().unwrap();
    }
    let copy = find_verb(session.menu(), "copy").unwrap().id.clone();
    let started = Instant::now();
    session
        .invoke(&copy, InvokeOptions::default())
        .wait_timeout(ANSWER)
        .unwrap()
        .unwrap();
    eprintln!("invoke copy: {} ms", started.elapsed().as_millis());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !clipboard_has_file_drop() && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(clipboard_has_file_drop());
}

#[test]
fn an_unknown_verb_without_an_id_fails_without_a_retry() {
    let fixture = Fixture::new();
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, vec![fixture.file("notes.txt")]);
    let unknown = ItemId::Shell {
        command: 0,
        verb: Some("ferrite-no-such-verb".to_owned()),
    };
    let result = session
        .invoke(&unknown, InvokeOptions::default())
        .wait_timeout(ANSWER)
        .unwrap();
    eprintln!("unknown verb: {result:?}");
    assert!(matches!(result, Err(MenuError::Shell { .. })), "{result:?}");
}

fn filled(session: &MenuSession, position: usize) -> Vec<MenuItem> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match session.fill(&[position]).wait_timeout(ANSWER).unwrap() {
            Ok(Submenu::Filled(children)) => return children,
            Ok(Submenu::Loading) if Instant::now() < deadline => std::thread::sleep(POLL),
            other => panic!("the submenu did not fill: {other:?}"),
        }
    }
}

fn files_in(folder: &Path) -> Vec<String> {
    std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_folder_background_menu_creates_a_text_document_from_new() {
    let fixture = Fixture::new();
    let folder = fixture.folder("drafts");
    let service = ShellMenuService::start().unwrap();
    let started = Instant::now();
    let session = service
        .open_background(folder.clone())
        .wait_timeout(ANSWER)
        .expect("the shell answers")
        .expect("the background menu opens");
    let names: Vec<_> = session.menu().items.iter().map(|item| &item.text).collect();
    eprintln!(
        "background: {names:?} in {} ms",
        started.elapsed().as_millis()
    );
    let position = session
        .menu()
        .items
        .iter()
        .position(|item| item.is_submenu() && item.text.eq_ignore_ascii_case("New"))
        .expect("a New submenu");
    let children = filled(&session, position);
    let kinds: Vec<_> = children.iter().map(|child| &child.text).collect();
    eprintln!("new: {kinds:?}");
    let text_document = children
        .iter()
        .find(|child| child.text.contains("Text Document"))
        .expect("a Text Document item")
        .id
        .clone();
    session
        .invoke(&text_document, InvokeOptions::default())
        .wait_timeout(ANSWER)
        .unwrap()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while files_in(&folder).is_empty() && Instant::now() < deadline {
        std::thread::sleep(POLL);
    }
    let created = files_in(&folder);
    eprintln!("created: {created:?}");
    assert!(
        created.iter().any(|name| name.ends_with(".txt")),
        "{created:?}"
    );
}

#[test]
fn items_in_two_folders_give_different_folders() {
    let (first, second) = (Fixture::new(), Fixture::new());
    let service = ShellMenuService::start().unwrap();
    let result = service
        .open(vec![first.file("a.txt"), second.file("b.txt")])
        .wait_timeout(ANSWER)
        .unwrap();
    assert_eq!(result.err(), Some(MenuError::DifferentFolders));
}

#[derive(Debug, Clone, Copy)]
struct Resources {
    user: u32,
    gdi: u32,
    handles: u32,
}

fn resources() -> Resources {
    // SAFETY: queries about the current process only.
    unsafe {
        let process = GetCurrentProcess();
        let mut handles = 0;
        GetProcessHandleCount(process, &mut handles).unwrap();
        Resources {
            user: GetGuiResources(process, GR_USEROBJECTS),
            gdi: GetGuiResources(process, GR_GDIOBJECTS),
            handles,
        }
    }
}

fn open_and_drop(service: &ShellMenuService, path: &Path, times: usize) {
    for _ in 0..times {
        drop(
            service
                .open(vec![path.to_path_buf()])
                .wait_timeout(ANSWER)
                .unwrap()
                .unwrap(),
        );
    }
}

#[test]
fn dropped_sessions_release_their_menus() {
    let fixture = Fixture::new();
    let path = fixture.file("notes.txt");
    let service = ShellMenuService::start().unwrap();
    open_and_drop(&service, &path, 10);
    let before = resources();
    let started = Instant::now();
    open_and_drop(&service, &path, 200);
    let elapsed = started.elapsed();
    let after = resources();
    eprintln!(
        "200 opens in {} ms; before {before:?}, after {after:?}",
        elapsed.as_millis()
    );
    assert!(after.user <= before.user + 20, "USER objects grew");
    assert!(after.gdi <= before.gdi + 40, "GDI objects grew");
    assert!(after.handles <= before.handles + 200, "kernel handles grew");
}

fn close_visible_menu(deadline: Instant) -> bool {
    // SAFETY: read-only window queries and a posted key message to this process's menu window.
    unsafe {
        let pid = std::process::id();
        while Instant::now() < deadline {
            let mut after: Option<HWND> = None;
            while let Ok(window) = FindWindowExW(None, after, w!("#32768"), None) {
                let mut owner = 0;
                GetWindowThreadProcessId(window, Some(&mut owner));
                if owner == pid && IsWindowVisible(window).as_bool() {
                    let escape = WPARAM(0x1B);
                    return PostMessageW(Some(window), WM_KEYDOWN, escape, LPARAM(0)).is_ok();
                }
                after = Some(window);
            }
            std::thread::yield_now();
        }
        false
    }
}

#[test]
fn the_classic_menu_opens_and_closes_on_escape() {
    let fixture = Fixture::new();
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, vec![fixture.file("notes.txt")]);
    let closer =
        std::thread::spawn(|| close_visible_menu(Instant::now() + Duration::from_secs(10)));
    let point = ScreenPoint { x: 200, y: 200 };
    let result = session
        .classic(WindowHandle(0), point)
        .wait_timeout(ANSWER)
        .unwrap();
    let closed = closer.join().unwrap();
    eprintln!("classic: {result:?}, menu window seen: {closed}");
    assert_eq!(result, Ok(None));
    assert!(closed, "the classic menu window never appeared");
}
