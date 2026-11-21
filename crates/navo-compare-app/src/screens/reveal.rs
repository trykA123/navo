//! Reveal in Explorer (NC138): builds the one argument for `explorer.exe /select,` from a
//! checked path and spawns Explorer on Windows. Used by the Folders context menu.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// The toast text off Windows, where there is no Explorer to open.
#[cfg(not(windows))]
pub const OFF_WINDOWS: &str = "Reveal in Explorer works on Windows only.";

/// The `/select,"<path>"` argument for `explorer.exe`. Fails with a reason when the path is not
/// absolute, does not exist, is not UTF-8, or holds `"` or a control character (U+0000-U+001F,
/// U+007F), because the text goes to Explorer unescaped.
pub fn select_arg(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("the path is not absolute".to_owned());
    }
    let text = path.to_str().ok_or("the path is not valid UTF-8")?;
    if text.chars().any(|c| c == '"' || c.is_control()) {
        return Err("the path holds a quote or a control character".to_owned());
    }
    if !path.exists() {
        return Err("the path does not exist".to_owned());
    }
    Ok(format!("/select,\"{text}\""))
}

/// The full path of `explorer.exe` under `SystemRoot`, so Windows never picks a copy that sits
/// next to our own exe. Compiles on every OS; the fallback is `C:\Windows`.
fn explorer_exe() -> PathBuf {
    explorer_in(std::env::var_os("SystemRoot").as_deref())
}

/// `explorer.exe` under `system_root`, or under `C:\Windows` when it is unset or not absolute.
fn explorer_in(system_root: Option<&OsStr>) -> PathBuf {
    let root = system_root.map(Path::new).filter(|root| root.is_absolute());
    root.unwrap_or(Path::new(r"C:\Windows"))
        .join("explorer.exe")
}

/// Opens Explorer with `path` selected and does not wait for it. Fails with the user-facing
/// toast text: a refused path, a spawn error, or the off-Windows notice.
#[cfg(windows)]
pub fn reveal(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt as _;
    let arg = select_arg(path).map_err(|why| format!("Could not open Explorer: {why}."))?;
    std::process::Command::new(explorer_exe())
        .raw_arg(arg)
        .spawn()
        .map(drop)
        .map_err(|error| format!("Could not open Explorer: {error}."))
}

/// Off Windows there is no Explorer: always the notice.
#[cfg(not(windows))]
pub fn reveal(_path: &Path) -> Result<(), String> {
    Err(OFF_WINDOWS.to_owned())
}

#[cfg(test)]
#[path = "reveal_tests.rs"]
mod tests;
