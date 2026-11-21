//! The update check's machine state: launch count, last check, last result, latest and hidden version.

use std::fs::{self, File};
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::paths::Profile;

/// Largest state file read by `load`; bigger files give the defaults.
pub const MAX_STATE_BYTES: u64 = 16 * 1024;

/// What the update check remembers between launches. Times are Unix seconds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateState {
    /// Number of launches seen.
    pub launches: u32,
    /// Unix time of the last check.
    pub last_check: Option<i64>,
    /// Outcome text of the last check.
    pub last_result: Option<String>,
    /// Latest version found.
    pub latest: Option<String>,
    /// Version the user hid.
    pub dismissed: Option<String>,
}

/// Loads the state; a missing, oversized, non-UTF-8 or unparsable file gives the defaults.
pub fn load(profile: &Profile) -> UpdateState {
    let Ok(file) = File::open(profile.update_state_file()) else {
        return UpdateState::default();
    };
    if file
        .metadata()
        .map_or(true, |meta| meta.len() > MAX_STATE_BYTES)
    {
        return UpdateState::default();
    }
    let mut bytes = Vec::new();
    if file
        .take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_STATE_BYTES
    {
        return UpdateState::default();
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return UpdateState::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

/// Saves the state through a temporary file, syncs it and renames it into place.
///
/// Fails when the state cannot be encoded or a file operation fails.
pub fn save(profile: &Profile, state: &UpdateState) -> std::io::Result<()> {
    let path = profile.update_state_file();
    let text = toml::to_string(state)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let temp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    let mut file = File::create(&temp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, &path)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::paths::Folders;

    /// A temp folder holding a profile, removed when the test ends.
    struct Temp {
        root: PathBuf,
        profile: Profile,
    }

    impl Temp {
        /// Opens a fresh profile under the system temporary folder.
        fn new(name: &str) -> Temp {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let count = COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "{}-ustate-{}-{name}-{count}",
                crate::brand::SLUG,
                std::process::id()
            ));
            let profile = Profile::open(Folders {
                config: root.join("config"),
                data: root.join("data"),
            })
            .expect("profile");
            Temp { root, profile }
        }
    }

    impl Drop for Temp {
        /// Removes the folder, ignoring errors.
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// Returns a state with every field set.
    fn full() -> UpdateState {
        UpdateState {
            launches: 7,
            last_check: Some(1_700_000_000),
            last_result: Some("ok".to_owned()),
            latest: Some("0.2.0".to_owned()),
            dismissed: Some("0.1.9".to_owned()),
        }
    }

    #[test]
    fn full_state_round_trips() {
        let temp = Temp::new("round");
        save(&temp.profile, &full()).expect("save");
        assert_eq!(load(&temp.profile), full());
    }

    #[test]
    fn missing_file_gives_default() {
        let temp = Temp::new("missing");
        assert_eq!(load(&temp.profile), UpdateState::default());
    }

    #[test]
    fn oversized_file_gives_default_and_stays() {
        let temp = Temp::new("big");
        let path = temp.profile.update_state_file();
        let text = format!("launches = 3\n#{}\n", "x".repeat(17 * 1024));
        fs::write(&path, &text).expect("write");
        assert_eq!(load(&temp.profile), UpdateState::default());
        assert_eq!(fs::read_to_string(&path).expect("read"), text);
    }

    #[test]
    fn bad_type_and_non_utf8_give_default() {
        let temp = Temp::new("bad");
        let path = temp.profile.update_state_file();
        fs::write(&path, "last_check = \"x\"\n").expect("write");
        assert_eq!(load(&temp.profile), UpdateState::default());
        fs::write(&path, [0xff, 0xfe, 0x00]).expect("write");
        assert_eq!(load(&temp.profile), UpdateState::default());
    }

    #[test]
    fn save_leaves_no_temp_file_in_the_data_folder() {
        let temp = Temp::new("tmp");
        save(&temp.profile, &full()).expect("save");
        let data = temp.root.join("data");
        let names: Vec<String> = fs::read_dir(&data)
            .expect("dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(
            names.iter().all(|name| !name.ends_with(".tmp")),
            "{names:?}"
        );
        assert!(data.join("update-check.toml").is_file());
        assert!(!temp.root.join("config").join("update-check.toml").exists());
    }
}
