#![cfg(windows)]
#![allow(clippy::print_stderr, reason = "menus are printed for the CI log")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on a broken fixture"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusty_core::shell_menu::{ItemId, MenuItem, ShellMenu, Submenu};
use rusty_shell::{
    InvokeOptions, MenuPaths, MenuSession, ScreenPoint, ShellMenuService, WindowHandle,
};
use tempfile::TempDir;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_KEYDOWN,
};
use windows::core::w;

const ANSWER: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(10);

fn temp_file(dir: &TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, b"ferrite shell menu test").unwrap();
    path
}

fn open(service: &ShellMenuService, path: &Path, extended: bool) -> MenuSession {
    let paths = MenuPaths::Items(vec![path.to_path_buf()]);
    service
        .open_paths(paths, extended)
        .wait_timeout(ANSWER)
        .expect("the shell answers")
        .expect("the menu opens")
}

fn verbs(items: &[MenuItem], into: &mut BTreeSet<String>) {
    for item in items {
        if let ItemId::Shell {
            verb: Some(verb), ..
        } = &item.id
        {
            into.insert(verb.to_ascii_lowercase());
        }
        verbs(item.children().unwrap_or_default(), into);
    }
}

fn verbs_of(menu: &ShellMenu) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    verbs(&menu.items, &mut found);
    found
}

#[expect(clippy::panic, reason = "a test helper fails the calling test")]
fn send_to(session: &MenuSession) -> Vec<MenuItem> {
    let position = session
        .menu()
        .items
        .iter()
        .position(|item| item.text.eq_ignore_ascii_case("Send to") && item.is_submenu())
        .expect("a Send to submenu");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match session.fill(&[position]).wait_timeout(ANSWER).unwrap() {
            Ok(Submenu::Filled(children)) => return children,
            Ok(Submenu::Loading) if Instant::now() < deadline => std::thread::sleep(POLL),
            other => panic!("Send to did not fill: {other:?}"),
        }
    }
}

#[test]
fn shift_adds_the_extended_verbs() {
    let dir = tempfile::tempdir().unwrap();
    let file = temp_file(&dir, "notes.txt");
    let service = ShellMenuService::start().unwrap();
    let plain = verbs_of(open(&service, &file, false).menu());
    let extended = verbs_of(open(&service, &file, true).menu());
    eprintln!("plain: {plain:?}\nextended: {extended:?}");
    assert!(plain.is_subset(&extended), "{plain:?} vs {extended:?}");
    assert!(
        extended.contains("copyaspath") || extended.contains("runas"),
        "{extended:?}"
    );
}

#[test]
fn send_to_targets_carry_icons() {
    let dir = tempfile::tempdir().unwrap();
    let file = temp_file(&dir, "notes.txt");
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, &file, false);
    let targets = send_to(&session);
    let with_icons = targets.iter().filter(|item| item.icon.is_some()).count();
    eprintln!(
        "send to: {} targets, {with_icons} with icons",
        targets.len()
    );
    assert!(with_icons > 0, "{targets:?}");
}

#[test]
fn send_to_compressed_folder_creates_a_zip() {
    let dir = tempfile::tempdir().unwrap();
    let file = temp_file(&dir, "notes.txt");
    let service = ShellMenuService::start().unwrap();
    let session = open(&service, &file, false);
    let zipped = send_to(&session)
        .into_iter()
        .find(|item| item.text.contains("zipped"))
        .expect("a Compressed (zipped) folder target");
    session
        .invoke(&zipped.id, InvokeOptions::default())
        .wait_timeout(ANSWER)
        .unwrap()
        .unwrap();
    let zip = dir.path().join("notes.zip");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !zip.exists() && Instant::now() < deadline {
        std::thread::sleep(POLL);
    }
    assert!(zip.exists(), "no zip beside the file");
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
fn the_classic_menu_without_a_session_closes_on_escape() {
    let dir = tempfile::tempdir().unwrap();
    let file = temp_file(&dir, "notes.txt");
    let service = ShellMenuService::start().unwrap();
    let closer =
        std::thread::spawn(|| close_visible_menu(Instant::now() + Duration::from_secs(10)));
    let point = ScreenPoint { x: 200, y: 200 };
    let result = service
        .classic_for(
            MenuPaths::Items(vec![file]),
            false,
            (WindowHandle(0), point),
        )
        .wait_timeout(ANSWER)
        .unwrap();
    let closed = closer.join().unwrap();
    eprintln!("classic without a session: {result:?}, menu window seen: {closed}");
    assert_eq!(result, Ok(None));
    assert!(closed, "the classic menu window never appeared");
}
