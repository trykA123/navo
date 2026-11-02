use std::path::Path;

use super::system::{unix_terminal, windows_provider};
use super::{command_line, launch_command, split, windows_creation_flags};

#[test]
fn windows_terminal_never_puts_the_folder_in_arguments() {
    let folder = Path::new(r"C:\Example Folder;calc.exe\docs");
    let (program, args) = command_line("windows-terminal", None, folder).unwrap();
    assert_eq!(program, "wt.exe");
    assert_eq!(args, ["-d", "."]);
}

#[test]
fn powershell_uses_the_available_executable_without_a_folder_argument() {
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
    assert_eq!(args, ["-NoExit"]);
}

#[test]
fn console_providers_never_put_folder_metacharacters_in_arguments() {
    let folder = Path::new(r"C:\x&calc|^%$;` folder");
    for (provider, expected) in [("cmd", vec![]), ("powershell", vec!["-NoExit"])] {
        let (_, args) = command_line(provider, None, folder).unwrap();
        assert_eq!(args, expected);
        let command = launch_command(provider, None, folder).unwrap();
        assert_eq!(command.get_current_dir(), Some(folder));
        assert!(
            command
                .get_args()
                .all(|arg| !arg.to_string_lossy().contains("x&calc"))
        );
    }
}

#[test]
fn windows_console_providers_get_a_new_console_instead_of_detaching() {
    for provider in [
        "cmd",
        "powershell",
        "pwsh",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "cmd.exe.",
        "CMD.EXE  ",
        "cmd.com",
        r"C:\Windows\System32\CMD.EXE",
    ] {
        assert_eq!(windows_creation_flags(provider), 0x10 | 0x200);
        assert_eq!(windows_creation_flags(provider) & 0x8, 0);
    }
    assert_eq!(windows_creation_flags("windows-terminal"), 0x8 | 0x200);
    assert_eq!(windows_creation_flags("wt.exe"), 0x8 | 0x200);
}

#[test]
fn custom_templates_split_before_replacing_paths() {
    let (program, args) = command_line(
        "custom",
        Some(r#""C:\Terminal App\terminal.exe" --cwd {path} """#),
        Path::new(r"C:\Example &|^%$;` Folder\"),
    )
    .unwrap();
    assert_eq!(program, r"C:\Terminal App\terminal.exe");
    assert_eq!(args, ["--cwd", r"C:\Example &|^%$;` Folder\", ""]);
}

#[test]
fn custom_windows_terminal_escapes_every_folder_semicolon() {
    let folder = Path::new(r"C:\safe;calc.exe; folder");
    for template in [
        "wt -d {path}",
        "WT.EXE -d {path}",
        r#""C:\Terminal App\Wt.ExE" -d {path}"#,
        "/terminal/WT.exe -d {path}",
    ] {
        let (_, args) = command_line("custom", Some(template), folder).unwrap();
        assert_eq!(args, ["-d", r"C:\safe\;calc.exe\; folder"]);
    }
}

#[test]
fn custom_non_shell_programs_accept_folder_substitution() {
    let folder = Path::new(r"C:\Example Folder");
    for template in ["code {path}", "wt -d {path}"] {
        let (_, args) = command_line("custom", Some(template), folder).unwrap();
        assert_eq!(args.last().unwrap(), folder.to_str().unwrap());
    }
}

#[test]
fn custom_templates_reject_folder_substitution_in_shell_commands() {
    for template in [
        r#"terminal --command "cd {path}""#,
        "terminal --cwd={path}",
        "{path} --new-window",
        "cmd.exe /K cd /d {path}",
        "wt.exe cmd.exe /K cd /d {path}",
        r#""C:\Windows\System32\CMD.EXE" /K {path}"#,
        "powershell.exe -Command {path}",
        "pwsh -Command {path}",
        "sh -c {path}",
        r"C:\bin\BASH.EXE -c {path}",
        "cmd.exe. /c {path}",
        "CMD.EXE   /c {path}",
        r#""CMD.EXE  " /c {path}"#,
        r#""C:\Windows\System32\cmd.exe" /c {path}"#,
        "x.bat {path}",
        "x.CMD. {path}",
        "pwsh.EXE -c {path}",
        "cmd.com /c {path}",
        "wsl {path}",
        "wscript {path}",
        "cscript {path}",
        "mshta {path}",
        "conhost {path}",
        "wt cmd.exe. /c {path}",
    ] {
        assert!(
            command_line(
                "custom",
                Some(template),
                Path::new(r"C:\x&calc|^%$;` folder")
            )
            .is_err()
        );
    }
}

#[test]
fn custom_template_without_a_path_still_sets_the_working_directory() {
    let folder = Path::new("example folder");
    for (template, program, argument) in [
        ("terminal --new-window", "terminal", "--new-window"),
        ("cmd.exe. /c", "cmd.exe.", "/c"),
        (r#""CMD.EXE  " /c"#, "CMD.EXE  ", "/c"),
    ] {
        let command = launch_command("custom", Some(template), folder).unwrap();
        assert_eq!(command.get_current_dir(), Some(folder));
        assert_eq!(command.get_program(), program);
        assert_eq!(command.get_args().collect::<Vec<_>>(), [argument]);
    }
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
    assert!(args.is_empty());
}
