use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use rusty_fs::Watch;
use rusty_ui::settings::{Change, Value, Values};
use rusty_ui::tokens::Density;
use rusty_ui::window_view::{SettingsHooks, SettingsNews};

use super::{Loaded, watched};

const NEWS_WAIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_secs(2);
const NORMAL: &str = "# mine\n[appearance]\ndensity = \"normal\"\n";
const COMPACT: &str = "# mine\n[appearance]\ndensity = \"compact\"\n";
const BROKEN: &str = "# mine\n[appearance]\ndensity = \n";

#[test]
fn module_prefixes_and_rename_logging_follow_live_settings() {
    let live = live_on("[automotive]\nmodules = [\"Com\"]\n");
    assert_eq!(live.hooks.values.module_prefixes(), ["Com"]);
    live.write("[automotive]\nmodules = [\"Dio_Cfg\"]\n[files]\nrename-log = true\n");
    let values = applied(live.next());
    assert_eq!(values.module_prefixes(), ["Dio_Cfg"]);
    assert!(values.flag(rusty_ui::settings::Field::RenameLog));
}

struct Live {
    _dir: tempfile::TempDir,
    path: PathBuf,
    hooks: SettingsHooks,
    news: Receiver<SettingsNews>,
    _watching: Watch,
}

fn live_on(text: &str) -> Live {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("ferrite.toml");
    fs::write(&path, text).expect("write");
    let (sender, news) = mpsc::channel();
    let (hooks, watching) = watched(Some(Loaded::load(&path)), move |news| {
        let _ = sender.send(news);
    });
    let watching = watching.expect("the settings folder is watched");
    let deadline = Instant::now() + NEWS_WAIT;
    while !watching.is_live() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(watching.is_live());
    Live {
        _dir: dir,
        path,
        hooks,
        news,
        _watching: watching,
    }
}

impl Live {
    fn next(&self) -> SettingsNews {
        self.news.recv_timeout(NEWS_WAIT).expect("news")
    }

    fn is_quiet(&self) -> bool {
        self.news.recv_timeout(QUIET).is_err()
    }

    fn write(&self, text: &str) {
        fs::write(&self.path, text).expect("write");
    }
}

fn applied(news: SettingsNews) -> Values {
    match news {
        SettingsNews::Applied { values, .. } => values,
        SettingsNews::Invalid(message) => panic!("expected values, got: {message}"),
        SettingsNews::Defaults(_) => panic!("expected values, got a defaults notice"),
    }
}

fn invalid(news: SettingsNews) -> String {
    match news {
        SettingsNews::Invalid(message) => message,
        SettingsNews::Applied { .. } | SettingsNews::Defaults(_) => panic!("expected an error"),
    }
}

fn rename_over(path: &Path, text: &str) {
    let temp = path.with_file_name("ferrite.toml.swp");
    fs::write(&temp, text).expect("write temp");
    fs::rename(&temp, path).expect("rename over");
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_hand_edit_applies_its_value() {
    let live = live_on(NORMAL);
    assert_eq!(live.hooks.values.density(), Density::Normal);
    live.write(COMPACT);
    assert_eq!(applied(live.next()).density(), Density::Compact);
    assert!(live.is_quiet());
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn an_invalid_edit_keeps_the_last_settings_and_reports_its_line() {
    let live = live_on(NORMAL);
    live.write(COMPACT);
    assert_eq!(applied(live.next()).density(), Density::Compact);
    live.write(BROKEN);
    let message = invalid(live.next());
    assert!(message.starts_with("ferrite.toml"), "{message}");
    assert!(message.contains("line 3"), "{message}");
    let saved = (live.hooks.save)(&[Change::show_hidden(false)]);
    assert!(saved.error.is_some());
    assert_eq!(saved.values.density(), Density::Compact);
    assert_eq!(fs::read_to_string(&live.path).expect("read"), BROKEN);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_fixed_file_clears_the_error_even_with_the_old_values() {
    let live = live_on(NORMAL);
    live.write(BROKEN);
    invalid(live.next());
    live.write(NORMAL);
    assert_eq!(applied(live.next()).density(), Density::Normal);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_broken_file_at_start_reports_its_fix() {
    let live = live_on(BROKEN);
    assert!(live.hooks.load_error.is_some());
    live.write(COMPACT);
    assert_eq!(applied(live.next()).density(), Density::Compact);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_save_from_the_sheet_is_not_applied_again() {
    let live = live_on(NORMAL);
    let saved = (live.hooks.save)(&[Change::density(Density::Comfortable)]);
    assert_eq!(saved.error, None);
    let more = (live.hooks.save)(&[Change::show_hidden(false)]);
    assert_eq!(more.error, None);
    let mut applies = 0;
    while live.news.recv_timeout(QUIET).is_ok() {
        applies += 1;
    }
    assert_eq!(applies, 0);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn the_same_content_written_twice_applies_once() {
    let live = live_on(NORMAL);
    live.write(COMPACT);
    applied(live.next());
    live.write(COMPACT);
    assert!(live.is_quiet());
    live.write(BROKEN);
    invalid(live.next());
    live.write(BROKEN);
    assert!(live.is_quiet());
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn an_editor_rename_over_save_is_followed_every_time() {
    let live = live_on(NORMAL);
    rename_over(&live.path, COMPACT);
    assert_eq!(applied(live.next()).density(), Density::Compact);
    rename_over(&live.path, NORMAL);
    assert_eq!(applied(live.next()).density(), Density::Normal);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_key_deleted_by_hand_stays_deleted_and_its_default_applies() {
    let live = live_on("[files]\nshow-system = true\n");
    assert_eq!(
        live.hooks.values.get(rusty_ui::settings::Field::ShowSystem),
        &Value::Flag(true)
    );
    live.write("[files]\n");
    let values = applied(live.next());
    assert_eq!(
        values.get(rusty_ui::settings::Field::ShowSystem),
        Values::default().get(rusty_ui::settings::Field::ShowSystem)
    );
    assert!(live.is_quiet());
    assert_eq!(fs::read_to_string(&live.path).expect("read"), "[files]\n");
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn deleting_the_file_keeps_the_current_settings() {
    let live = live_on(COMPACT);
    assert_eq!(live.hooks.values.density(), Density::Compact);
    fs::remove_file(&live.path).expect("remove");
    assert!(live.is_quiet());
    live.write(NORMAL);
    assert_eq!(applied(live.next()).density(), Density::Normal);
}
