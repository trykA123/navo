use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_core::shell_menu::ShellMenu;
use rusty_ui::window_view::{Reply, ScreenSpot};

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

#[test]
fn a_file_outside_the_local_disk_gets_an_empty_menu_without_the_shell() {
    let provider = provider().expect("the menu service starts");
    let remote = Location::new("fake://home/a.txt").unwrap();
    let session = answer(&provider.open(&[remote])).expect("an empty menu");
    assert_eq!(*session.menu(), ShellMenu::default());
    let error = answer(&session.classic(ScreenSpot::default())).unwrap_err();
    assert_eq!(error, "the Windows menu works only on local files");
}

#[cfg(not(windows))]
mod linux {
    use rusty_core::Location;
    use rusty_core::shell_menu::{ItemId, ShellMenu};
    use rusty_ui::window_view::{ScreenSpot, ShellMenuSession};

    use super::{answer, provider};

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
}
