use std::fs;
use std::path::{Path, PathBuf};

use crate::editor::{Search, resolve};

/// Common Unix terminal emulators checked in priority order when `$TERMINAL` and desktop config
/// do not pick one.
const UNIX_TERMINALS: [&str; 22] = [
    "xdg-terminal-exec",
    "x-terminal-emulator",
    "ghostty",
    "kitty",
    "alacritty",
    "wezterm",
    "foot",
    "rio",
    "warp-terminal",
    "konsole",
    "gnome-terminal",
    "kgx",
    "ptyxis",
    "xfce4-terminal",
    "tilix",
    "terminator",
    "lxterminal",
    "mate-terminal",
    "urxvt",
    "st",
    "sakura",
    "xterm",
];
const NO_TERMINAL: &str = "x-terminal-emulator";
const CONSOLE_HOST: &str = "{B23D10C0-E52E-411E-9D5B-C09FDF709C7D}";

/// Known `.desktop` entry ids mapped to their terminal binary names.
const DESKTOP_TERMINALS: [(&str, &str); 18] = [
    ("com.mitchellh.ghostty.desktop", "ghostty"),
    ("ghostty.desktop", "ghostty"),
    ("kitty.desktop", "kitty"),
    ("Alacritty.desktop", "alacritty"),
    ("alacritty.desktop", "alacritty"),
    ("org.wezfurlong.wezterm.desktop", "wezterm"),
    ("wezterm.desktop", "wezterm"),
    ("foot.desktop", "foot"),
    ("footclient.desktop", "foot"),
    ("com.raphamorim.rio.desktop", "rio"),
    ("org.kde.konsole.desktop", "konsole"),
    ("org.gnome.Terminal.desktop", "gnome-terminal"),
    ("org.gnome.Console.desktop", "kgx"),
    ("org.gnome.Ptyxis.desktop", "ptyxis"),
    ("xfce4-terminal.desktop", "xfce4-terminal"),
    ("com.gexperts.Tilix.desktop", "tilix"),
    ("terminator.desktop", "terminator"),
    ("xterm.desktop", "xterm"),
];

#[cfg(test)]
pub(super) fn unix_terminal(terminal: Option<&str>, on_path: impl Fn(&str) -> bool) -> String {
    unix_terminal_with_desktop(terminal, &[], on_path)
}

pub(super) fn unix_terminal_with_desktop(
    terminal: Option<&str>,
    desktop_candidates: &[String],
    on_path: impl Fn(&str) -> bool,
) -> String {
    let set = terminal
        .map(str::trim)
        .filter(|terminal| !terminal.is_empty());
    if let Some(chosen) = set {
        return chosen.to_owned();
    }
    if on_path("xdg-terminal-exec") {
        return "xdg-terminal-exec".to_owned();
    }
    if let Some(chosen) = desktop_candidates
        .iter()
        .find(|candidate| candidate_available(candidate, &on_path))
    {
        return chosen.clone();
    }
    UNIX_TERMINALS
        .iter()
        .find(|name| on_path(name))
        .map_or_else(|| NO_TERMINAL.to_owned(), |name| (*name).to_owned())
}

fn candidate_available(command: &str, on_path: &impl Fn(&str) -> bool) -> bool {
    let Some(program) = command.split_whitespace().next() else {
        return false;
    };
    if Path::new(program).is_absolute() {
        Path::new(program).is_file()
    } else {
        on_path(program)
    }
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

pub(super) fn terminal_command(
    folder: &Path,
) -> Result<(String, Vec<String>), super::TerminalError> {
    let (provider, template) = discover();
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
    let env_terminal = std::env::var("TERMINAL").ok();
    if cfg!(target_os = "macos") {
        let chosen = env_terminal
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map_or_else(|| "open -a Terminal .".to_owned(), str::to_owned);
        return ("custom", Some(chosen));
    }
    let search = Search {
        path,
        extensions: String::new(),
    };
    let desktop = desktop_terminal_candidates();
    let terminal = unix_terminal_with_desktop(env_terminal.as_deref(), &desktop, |name| {
        resolve(name, &search).is_some()
    });
    ("custom", Some(terminal))
}

fn desktop_terminal_candidates() -> Vec<String> {
    let mut out = Vec::new();
    let Some(config_dir) = xdg_config_home() else {
        return out;
    };
    let desktops = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    for desktop in desktops
        .split(':')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let file = config_dir.join(format!(
            "{}-xdg-terminals.list",
            desktop.to_ascii_lowercase()
        ));
        collect_xdg_terminals_list(&file, &mut out);
    }
    collect_xdg_terminals_list(&config_dir.join("xdg-terminals.list"), &mut out);
    collect_kde_terminal(&config_dir.join("kdeglobals"), &mut out);
    out
}

fn xdg_config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

pub(super) fn parse_xdg_terminals_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(desktop_id_to_binary)
        .map(str::to_owned)
        .collect()
}

fn collect_xdg_terminals_list(path: &Path, out: &mut Vec<String>) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    out.extend(parse_xdg_terminals_list(&text));
}

pub(super) fn parse_kde_terminal(text: &str) -> Option<String> {
    let mut in_general = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_general = line.eq_ignore_ascii_case("[General]");
            continue;
        }
        if !in_general {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("TerminalApplication")
        {
            let chosen = value.trim();
            if !chosen.is_empty() {
                return Some(chosen.to_owned());
            }
        }
    }
    None
}

fn collect_kde_terminal(path: &Path, out: &mut Vec<String>) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    if let Some(terminal) = parse_kde_terminal(&text) {
        out.push(terminal);
    }
}

fn desktop_id_to_binary(entry: &str) -> Option<&'static str> {
    let desktop_id = entry.split(':').next()?.trim();
    DESKTOP_TERMINALS
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(desktop_id))
        .map(|(_, bin)| *bin)
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
