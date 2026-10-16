#![cfg(not(windows))]

use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_core::shell_menu::{ItemId, ShellMenu};
use rusty_ui::window_view::{Reply, ScreenSpot, ShellMenuSession};

use super::provider;

fn answer<T>(reply: &Reply<T>) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(answer) = reply() {
            return answer;
        }
        assert!(Instant::now() < deadline, "the shell never answered");
        std::thread::yield_now();
    }
}

fn session() -> Box<dyn ShellMenuSession> {
    let provider = provider().expect("the menu service starts");
    let file = Location::new("/home/admin/notes.txt").unwrap();
    answer(&provider.open(&[file])).expect("a one-folder selection opens")
}

#[test]
fn the_linux_menu_has_only_ferrite_actions() {
    assert_eq!(*session().menu(), ShellMenu::default());
}

#[test]
fn shell_commands_report_that_they_are_unsupported() {
    let id = ItemId::Shell {
        command: 1,
        verb: Some("open".to_owned()),
    };
    let error = answer(&session().invoke(&id, ScreenSpot::default())).unwrap_err();
    assert_eq!(error, "shell menus are not supported on this platform");
}
