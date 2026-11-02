use std::path::Path;
use std::sync::OnceLock;

use crate::editor::{Search, resolve};

const UNIX_TERMINALS: [&str; 9] = [
    "xdg-terminal-exec",
    "x-terminal-emulator",
    "kitty",
    "alacritty",
    "foot",
    "wezterm",
    "gnome-terminal",
    "konsole",
    "xfce4-terminal",
];
const NO_TERMINAL: &str = "x-terminal-emulator";
const CONSOLE_HOST: &str = "{B23D10C0-E52E-411E-9D5B-C09FDF709C7D}";

pub(super) fn unix_terminal(terminal: Option<&str>, on_path: impl Fn(&str) -> bool) -> String {
    let set = terminal
        .map(str::trim)
        .filter(|terminal| !terminal.is_empty());
    set.map(str::to_owned)
        .or_else(|| {
            UNIX_TERMINALS
                .iter()
                .find(|name| on_path(name))
                .map(|name| (*name).to_owned())
        })
        .unwrap_or_else(|| NO_TERMINAL.to_owned())
}

pub(super) fn windows_provider(
    delegation: Option<&str>,
    has_windows_terminal: bool,
) -> &'static str {
    let chosen = delegation.map(str::trim).unwrap_or_default();
    if chosen.eq_ignore_ascii_case(CONSOLE_HOST) || !has_windows_terminal {
        "cmd"
    } else {
        "windows-terminal"
    }
}

pub(super) fn terminal_command(folder: &Path) -> Result<(String, Vec<String>), String> {
    static CHOSEN: OnceLock<(&str, Option<String>)> = OnceLock::new();
    let (provider, template) = CHOSEN.get_or_init(discover);
    super::command_line(provider, template.as_deref(), folder)
}

fn discover() -> (&'static str, Option<String>) {
    let path = std::env::var("PATH").unwrap_or_default();
    if cfg!(windows) {
        return (
            windows_provider(delegation().as_deref(), has_windows_terminal(&path)),
            None,
        );
    }
    let search = Search {
        path,
        extensions: String::new(),
    };
    let terminal = unix_terminal(std::env::var("TERMINAL").ok().as_deref(), |name| {
        resolve(name, &search).is_some()
    });
    ("custom", Some(terminal))
}

fn has_windows_terminal(path: &str) -> bool {
    let search = Search {
        path: path.to_owned(),
        extensions: ".EXE".to_owned(),
    };
    let alias = std::env::var_os("LOCALAPPDATA")
        .map(|root| Path::new(&root).join(r"Microsoft\WindowsApps\wt.exe"));
    resolve("wt.exe", &search).is_some()
        || alias.is_some_and(|alias| alias.symlink_metadata().is_ok())
}

#[cfg(windows)]
fn delegation() -> Option<String> {
    registry::text(r"Console\%%Startup", "DelegationTerminal")
}

#[cfg(not(windows))]
fn delegation() -> Option<String> {
    None
}

#[cfg(windows)]
mod registry {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};

    const UNITS: usize = 128;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub(super) fn text(key: &str, name: &str) -> Option<String> {
        let (key, name) = (wide(key), wide(name));
        let mut buffer = [0_u16; UNITS];
        let mut size = (UNITS * 2) as u32;
        // SAFETY: the key and value names are NUL-terminated, and `size` is the byte length of the writable `buffer`.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                buffer.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let units = (size as usize / 2).min(UNITS);
        let text = String::from_utf16_lossy(&buffer[..units]);
        Some(text.trim_end_matches('\0').to_owned())
    }
}
