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

fn delegation() -> Option<String> {
    navo_shell::registry_text(
        navo_shell::Hive::CurrentUser,
        r"Console\%%Startup",
        "DelegationTerminal",
    )
}
