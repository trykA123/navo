use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_core::shell_menu::ShellMenu;
use rusty_ui::window_view::{Reply, ScreenSpot};

use super::{provider, shell_form};

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

#[test]
fn the_shell_gets_a_drive_path_without_the_verbatim_prefix() {
    let path = std::path::PathBuf::from(r"\\?\C:\WORK_STUFF\paperwing");
    assert_eq!(shell_form(path), Ok(r"C:\WORK_STUFF\paperwing".into()));
}

#[test]
fn the_shell_gets_a_share_path_without_the_verbatim_prefix() {
    let path = std::path::PathBuf::from(r"\\?\UNC\server\share\a.txt");
    assert_eq!(shell_form(path), Ok(r"\\server\share\a.txt".into()));
}

#[test]
fn a_plain_path_reaches_the_shell_unchanged() {
    let path = std::path::PathBuf::from("/home/admin/notes.txt");
    assert_eq!(shell_form(path.clone()), Ok(path));
}

#[test]
fn a_path_past_the_shell_limit_is_refused() {
    let long = format!(r"\\?\C:\{}", "a".repeat(300));
    let error = shell_form(std::path::PathBuf::from(long)).unwrap_err();
    assert_eq!(
        error,
        "the Windows menu cannot open paths longer than 259 characters"
    );
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

#[cfg(windows)]
#[test]
fn a_local_folder_opens_a_windows_menu() {
    let folder = std::env::temp_dir().join("ferrite-shell-path");
    std::fs::create_dir_all(&folder).expect("temp folder");
    let location = Location::new(folder.to_str().expect("utf-8")).expect("location");
    let provider = provider().expect("the menu service starts");
    let session = answer(&provider.open(&[location])).expect("a Windows menu");
    assert!(!session.menu().items.is_empty(), "no Windows items");
}
