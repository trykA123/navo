use std::path::Path;

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
fn linux_launches_the_environment_terminal_or_system_default() {
    let folder = Path::new("example folder");
    let command = launch_command("windows-terminal", None, folder).unwrap();
    let expected = std::env::var("TERMINAL").unwrap_or_else(|_| "x-terminal-emulator".to_owned());
    let words = split(&expected).unwrap();
    assert_eq!(command.get_program(), words[0].as_str());
    assert_eq!(command.get_current_dir(), Some(folder));
}
