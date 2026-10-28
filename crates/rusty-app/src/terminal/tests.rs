use std::path::Path;

use super::system::{unix_terminal, windows_provider};
use super::{command_line, launch_command, split};

#[test]
fn terminal_providers_keep_a_folder_with_spaces_in_one_argument() {
    let folder = Path::new(r"C:\Example Folder\docs");
    for (provider, program, prefix) in [
        ("windows-terminal", "wt.exe", vec!["-d"]),
        ("cmd", "cmd.exe", vec!["/K", "cd", "/d"]),
    ] {
        let (actual, args) = command_line(provider, None, folder).unwrap();
        assert_eq!(actual, program);
        let expected: Vec<&str> = prefix
            .into_iter()
            .chain([folder.to_str().unwrap()])
            .collect();
        assert_eq!(args, expected);
    }
}

#[test]
fn powershell_uses_the_available_executable_and_working_directory_argument() {
    let folder = Path::new(r"C:\Example Folder");
    let (program, args) = command_line("powershell", None, folder).unwrap();
    let search = crate::editor::Search {
        path: std::env::var("PATH").unwrap_or_default(),
        extensions: ".EXE".to_owned(),
    };
    let expected = if crate::editor::resolve("pwsh.exe", &search).is_some() {
        "pwsh.exe"
    } else {
        "powershell.exe"
    };
    assert_eq!(program, expected);
    assert_eq!(args, ["-NoExit", "-WorkingDirectory", r"C:\Example Folder"]);
}

#[test]
fn custom_templates_split_before_replacing_paths() {
    let (program, args) = command_line(
        "custom",
        Some(r#""C:\Terminal App\terminal.exe" --cwd {path} """#),
        Path::new(r"C:\Example Folder\"),
    )
    .unwrap();
    assert_eq!(program, r"C:\Terminal App\terminal.exe");
    assert_eq!(args, ["--cwd", r"C:\Example Folder\", ""]);
}

#[test]
fn custom_template_without_a_path_still_sets_the_working_directory() {
    let folder = Path::new("example folder");
    let command = launch_command("custom", Some("terminal --new-window"), folder).unwrap();
    assert_eq!(command.get_current_dir(), Some(folder));
    assert_eq!(command.get_program(), "terminal");
    assert_eq!(command.get_args().collect::<Vec<_>>(), ["--new-window"]);
}

#[test]
fn windows_quoting_preserves_escaped_quotes_and_backslashes() {
    assert_eq!(
        split(r#"app "say \"hello\"" "C:\folder\\" C:\plain\ next"#).unwrap(),
        [
            "app",
            "say \"hello\"",
            "C:\\folder\\",
            "C:\\plain\\",
            "next"
        ]
    );
}

#[test]
fn invalid_custom_commands_return_errors() {
    for template in ["", "   ", "\"\"", "terminal \"unfinished"] {
        assert!(command_line("custom", Some(template), Path::new("folder")).is_err());
    }
    assert!(command_line("unknown", None, Path::new("folder")).is_err());
}

#[cfg(unix)]
#[test]
fn linux_launches_the_system_terminal_for_system_and_for_older_explicit_values() {
    let folder = Path::new("example folder");
    for provider in ["system", "windows-terminal"] {
        let command = launch_command(provider, None, folder).unwrap();
        assert_eq!(command.get_current_dir(), Some(folder));
        assert!(!command.get_program().is_empty());
    }
}

fn found(names: &[&str]) -> impl Fn(&str) -> bool {
    let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    move |name| names.iter().any(|found| found == name)
}

#[test]
fn unix_prefers_the_terminal_variable_over_everything_installed() {
    let everything = found(&["xdg-terminal-exec", "kitty", "konsole"]);
    assert_eq!(unix_terminal(Some("foot -1"), everything), "foot -1");
}

#[test]
fn unix_ignores_an_empty_terminal_variable() {
    assert_eq!(
        unix_terminal(Some("  "), found(&["xdg-terminal-exec"])),
        "xdg-terminal-exec"
    );
}

#[test]
fn unix_walks_the_list_in_order() {
    let order = [
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
    for at in 0..order.len() {
        assert_eq!(unix_terminal(None, found(&order[at..])), order[at]);
    }
}

#[test]
fn unix_without_any_terminal_still_names_the_system_alternative() {
    assert_eq!(unix_terminal(None, found(&[])), "x-terminal-emulator");
}

#[test]
fn windows_without_a_choice_uses_windows_terminal_when_installed_else_cmd() {
    for delegation in [
        None,
        Some(""),
        Some("{00000000-0000-0000-0000-000000000000}"),
        Some("{E12CFF52-A866-4C77-9A90-F570A7AA2C6B}"),
    ] {
        assert_eq!(windows_provider(delegation, true), "windows-terminal");
        assert_eq!(windows_provider(delegation, false), "cmd");
    }
}

#[test]
fn windows_console_host_chosen_by_the_user_opens_cmd() {
    let console_host = Some("{b23d10c0-e52e-411e-9d5b-c09fdf709c7d}");
    assert_eq!(windows_provider(console_host, true), "cmd");
    assert_eq!(windows_provider(console_host, false), "cmd");
}

#[test]
fn the_windows_choice_maps_to_the_folder_command() {
    let folder = Path::new(r"C:\Example Folder");
    let (program, args) = command_line(windows_provider(None, false), None, folder).unwrap();
    assert_eq!(program, "cmd.exe");
    assert_eq!(args, ["/K", "cd", "/d", r"C:\Example Folder"]);
}
