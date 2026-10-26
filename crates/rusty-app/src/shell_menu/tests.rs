use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_core::shell_menu::ShellMenu;
use rusty_ui::window_view::{MenuTarget, Reply, ScreenSpot};

use super::{provider, shell_form, started};

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
    let provider = started().expect("the menu service starts");
    let remote = Location::new("fake://home/a.txt").unwrap();
    let target = MenuTarget::Items(vec![remote]);
    let session = answer(&provider.open(&target, false)).expect("an empty menu");
    assert_eq!(*session.menu(), ShellMenu::default());
    let error = answer(&session.classic(ScreenSpot::default())).unwrap_err();
    assert_eq!(error, "the Windows menu works only on local files");
}

#[test]
fn the_classic_menu_of_a_file_outside_the_local_disk_is_refused() {
    let provider = started().expect("the menu service starts");
    let remote = Location::new("fake://home/a.txt").unwrap();
    let target = MenuTarget::Items(vec![remote]);
    let error = answer(&provider.classic_for(&target, false, ScreenSpot::default())).unwrap_err();
    assert_eq!(error, "the Windows menu works only on local files");
}

#[test]
fn a_folder_outside_the_local_disk_gets_an_empty_background_menu() {
    let provider = started().expect("the menu service starts");
    let remote = Location::new("fake://home").unwrap();
    let target = MenuTarget::Background(remote);
    let session = answer(&provider.open(&target, false)).expect("an empty menu");
    assert_eq!(*session.menu(), ShellMenu::default());
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

const RENAMED: &str = "the Windows menu cannot open names that Windows would change: \
     a trailing dot or space, or a device name such as NUL";

fn refused(text: &str) -> String {
    shell_form(std::path::PathBuf::from(text)).unwrap_err()
}

#[test]
fn a_name_ending_in_a_dot_is_refused() {
    assert_eq!(refused(r"\\?\C:\work\foo.\a.txt"), RENAMED);
}

#[test]
fn a_name_ending_in_a_space_is_refused() {
    assert_eq!(refused(r"\\?\C:\work\foo "), RENAMED);
}

#[test]
fn a_device_name_with_an_extension_is_refused() {
    assert_eq!(refused(r"\\?\C:\work\nul.txt"), RENAMED);
}

#[test]
fn a_numbered_device_name_is_refused_in_any_case() {
    assert_eq!(refused(r"\\?\C:\work\COM1"), RENAMED);
    assert_eq!(refused(r"\\?\C:\work\lpt9.log"), RENAMED);
}

#[test]
fn a_device_name_inside_a_share_path_is_refused() {
    assert_eq!(refused(r"\\?\UNC\server\share\aux\a.txt"), RENAMED);
}

#[test]
fn a_name_that_only_starts_like_a_device_reaches_the_shell() {
    let path = std::path::PathBuf::from(r"\\?\C:\work\console.txt");
    assert_eq!(shell_form(path), Ok(r"C:\work\console.txt".into()));
}

#[cfg(unix)]
mod raw_bytes {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;

    use super::{RENAMED, shell_form};

    fn raw(bytes: &[u8]) -> PathBuf {
        PathBuf::from(OsStr::from_bytes(bytes))
    }

    #[test]
    fn a_share_path_that_is_not_utf8_loses_the_verbatim_prefix() {
        let path = raw(b"\\\\?\\UNC\\server\\share\\menu\xe9.txt");
        assert_eq!(
            shell_form(path),
            Ok(raw(b"\\\\server\\share\\menu\xe9.txt"))
        );
    }

    #[test]
    fn a_drive_path_that_is_not_utf8_loses_the_verbatim_prefix() {
        let path = raw(b"\\\\?\\C:\\x\xff\\a.txt");
        assert_eq!(shell_form(path), Ok(raw(b"C:\\x\xff\\a.txt")));
    }

    #[test]
    fn a_name_that_is_not_utf8_and_ends_in_a_dot_is_refused() {
        let path = raw(b"\\\\?\\C:\\x\xff.\\a.txt");
        assert_eq!(shell_form(path).unwrap_err(), RENAMED);
    }
}

#[cfg(windows)]
mod wide {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use super::{RENAMED, shell_form};

    fn wide(text: &str, odd: u16, tail: &str) -> PathBuf {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        units.push(odd);
        units.extend(tail.encode_utf16());
        PathBuf::from(OsString::from_wide(&units))
    }

    #[test]
    fn a_share_path_with_a_lone_surrogate_loses_the_verbatim_prefix() {
        let path = wide(r"\\?\UNC\server\share\a", 0xD800, ".txt");
        let plain = wide(r"\\server\share\a", 0xD800, ".txt");
        assert_eq!(shell_form(path), Ok(plain));
    }

    #[test]
    fn a_name_with_a_lone_surrogate_and_a_trailing_space_is_refused() {
        let path = wide(r"\\?\C:\a", 0xD800, " ");
        assert_eq!(shell_form(path).unwrap_err(), RENAMED);
    }
}

#[cfg(not(windows))]
#[test]
fn without_windows_there_is_no_shell_menu_provider() {
    assert!(provider().is_none());
}

#[cfg(windows)]
#[test]
fn a_local_folder_opens_a_windows_menu() {
    let folder = std::env::temp_dir().join("ferrite-shell-path");
    std::fs::create_dir_all(&folder).expect("temp folder");
    let location = Location::new(folder.to_str().expect("utf-8")).expect("location");
    let provider = provider().expect("the menu service starts");
    let target = MenuTarget::Items(vec![location]);
    let session = answer(&provider.open(&target, true)).expect("a Windows menu");
    assert!(!session.menu().items.is_empty(), "no Windows items");
}
