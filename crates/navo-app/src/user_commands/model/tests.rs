#[cfg(unix)]
use std::path::Path;

use crate::editor::Search;

use super::super::program::{FileKind, with_probe};
use super::{
    Console, LineError, Part, Placeholder, Token, UserCommand, parse, parse_without_identity,
};

fn command(fields: &str) -> String {
    format!("[[command]]\nname = \"diff\"\nprogram = \"tool.exe\"\n{fields}")
}

fn with_program(program: &str, fields: &str) -> String {
    format!("[[command]]\nname = \"diff\"\nprogram = '{program}'\n{fields}")
}

fn loaded(text: &str) -> Vec<UserCommand> {
    parse_without_identity(text).expect("a valid command file")
}

fn only_error(text: &str) -> LineError {
    let mut errors = parse_without_identity(text).expect_err("a load error");
    assert_eq!(errors.len(), 1, "{errors:?}");
    errors.remove(0)
}

fn search() -> Search {
    Search {
        path: String::new(),
        extensions: String::new(),
    }
}

#[cfg(unix)]
fn search_in(folder: &Path) -> Search {
    Search {
        path: folder.to_string_lossy().into_owned(),
        extensions: String::new(),
    }
}

fn only_error_with(text: &str, search: &Search) -> LineError {
    let mut errors = parse(text, search).expect_err("a load error");
    assert_eq!(errors.len(), 1, "{errors:?}");
    errors.remove(0)
}

fn literal(token: &Token) -> String {
    token
        .parts
        .iter()
        .map(|part| match part {
            Part::Literal(text) => text.clone(),
            Part::Placeholder(placeholder) => format!("{{{}}}", placeholder.name()),
        })
        .collect()
}

fn placeholders(token: &Token) -> Vec<Placeholder> {
    token
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Placeholder(placeholder) => Some(*placeholder),
            Part::Literal(_) => None,
        })
        .collect()
}

#[test]
fn defaults_apply() {
    let commands = loaded("[[command]]\nname = \"diff\"\nprogram = \"tool.exe\"\n");
    assert_eq!(commands.len(), 1);
    let command = &commands[0];
    assert_eq!(command.name, "diff");
    assert_eq!(command.program, "tool.exe");
    assert!(command.args.is_empty());
    assert_eq!(
        command.start_in,
        [Token {
            parts: vec![Part::Placeholder(Placeholder::Folder)]
        }]
    );
    assert_eq!(command.console, Console::Show);
    assert_eq!(command.key, None);
}

#[test]
fn console_can_be_hidden() {
    let command = &loaded(&command("console = \"hide\"\n"))[0];
    assert_eq!(command.console, Console::Hide);
}

#[test]
fn every_placeholder_is_known() {
    let text = command(
        "args = '{file} {name} {stem} {ext} {folder} {other-folder} {other-file} {marked-list}'\n",
    );
    let command = &loaded(&text)[0];
    let found: Vec<Placeholder> = command.args.iter().flat_map(placeholders).collect();
    assert_eq!(
        found,
        [
            Placeholder::File,
            Placeholder::Name,
            Placeholder::Stem,
            Placeholder::Ext,
            Placeholder::Folder,
            Placeholder::OtherFolder,
            Placeholder::OtherFile,
            Placeholder::MarkedList,
        ]
    );
}

#[test]
fn marked_must_be_a_whole_argument() {
    let whole = &loaded(&command("args = '{marked}'\n"))[0];
    assert_eq!(
        whole.args[0].parts,
        [Part::Placeholder(Placeholder::Marked)]
    );
    let error = only_error(&command("args = 'pre{marked}post'\n"));
    assert_eq!(error.line, 4);
    assert!(error.message.contains("whole argument"), "{error}");
}

#[test]
fn literal_braces_write_braces() {
    let command = &loaded(&command("args = 'a{{b}}c'\n"))[0];
    assert_eq!(literal(&command.args[0]), "a{b}c");
}

#[test]
fn an_unknown_placeholder_names_its_line() {
    let error = only_error(&command("args = 'x {{ }} {what}'\n"));
    assert_eq!(error.line, 4);
    assert!(error.message.contains("{what}"), "{error}");
}

#[test]
fn an_unclosed_brace_is_an_error() {
    let error = only_error(&command("args = 'x {file'\n"));
    assert_eq!(error.line, 4);
}

#[test]
fn quotes_group_an_argument() {
    let command = &loaded(&command("args = '\"a b\" c'\n"))[0];
    assert_eq!(command.args.len(), 2);
    assert_eq!(literal(&command.args[0]), "a b");
    assert_eq!(literal(&command.args[1]), "c");
}

#[test]
fn a_duplicate_name_is_an_error() {
    let text = "[[command]]\nname = \"diff\"\nprogram = \"tool.exe\"\n\n\
                [[command]]\nname = \"diff\"\nprogram = \"other.exe\"\n";
    let error = only_error(text);
    assert_eq!(error.line, 6);
    assert!(error.message.contains("diff"), "{error}");
}

#[test]
fn a_placeholder_in_program_is_an_error() {
    let error = only_error("[[command]]\nname = \"x\"\nprogram = \"{file}\"\n");
    assert_eq!(error.line, 3);
    assert!(error.message.contains("program"), "{error}");
}

#[test]
fn a_bad_key_is_an_error() {
    for key in ["ctrl", "a", "7", "ctrl-a-b", "ctrl-k ctrl-c", ""] {
        let text = command(&format!("key = \"{key}\"\n"));
        let error = only_error(&text);
        assert_eq!(error.line, 4, "{key:?}");
        assert!(!error.message.is_empty(), "{key:?}");
    }
}

#[test]
fn a_reserved_key_is_an_error() {
    for key in [
        "alt-tab",
        "alt-escape",
        "ctrl-escape",
        "ctrl-shift-escape",
        "ctrl-alt-delete",
        "alt-f4",
        "super-r",
    ] {
        let text = command(&format!("key = \"{key}\"\n"));
        let error = only_error(&text);
        assert_eq!(error.line, 4, "{key:?}");
        assert!(error.message.contains("reserved"), "{key:?}: {error}");
    }
}

#[test]
fn a_key_is_normalized() {
    let command = &loaded(&command("key = \"Ctrl-Shift-E\"\n"))[0];
    assert_eq!(command.key.as_deref(), Some("ctrl-shift-e"));
}

#[test]
fn a_shell_command_may_not_use_placeholders() {
    let errors = [
        (with_program("cmd", ""), 1),
        (with_program("wt", "args = 'cmd /c {file}'\n"), 4),
        (with_program("wt", "args = '{folder} pwsh'\n"), 4),
        (with_program("tool.exe", "args = 'run.ps1 {file}'\n"), 4),
        (with_program(r"C:\x\go.bat", ""), 1),
        (
            "[[command]]\nname = \"x\"\nprogram = \"bash\"\nstart-in = \"{folder}\"\n".to_owned(),
            4,
        ),
    ];
    for (text, line) in errors {
        let error = only_error(&text);
        assert_eq!(error.line, line, "{text}");
        assert!(error.message.contains("shell"), "{text}: {error}");
    }
}

#[test]
fn a_quoted_shell_program_is_a_load_error() {
    let text = with_program("\"cmd.exe\"", "args = '{file}'\nstart-in = 'C:\\work'\n");
    let error = only_error(&text);
    assert!(error.message.contains("shell"), "{error}");
}

#[test]
fn a_script_word_with_a_trailing_dot_is_a_load_error() {
    let text = with_program(
        "tool.exe",
        "args = 'run.ps1. {file}'\nstart-in = 'C:\\work'\n",
    );
    let error = only_error(&text);
    assert!(error.message.contains("shell"), "{error}");
}

#[test]
fn a_terminal_command_with_a_placeholder_is_a_load_error() {
    let errors = [
        (
            with_program(
                "wt",
                "args = 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\POWERS~1.EXE -Command echo -d {stem}'\n",
            ),
            4,
        ),
        (
            with_program(
                "wt",
                "args = 'new-tab -p \"Windows PowerShell\" --appendCommandLine echo -d {stem}'\n",
            ),
            4,
        ),
        (with_program("wt", "args = '-d=.;{stem}'\n"), 4),
        (
            with_program(
                "wt",
                "args = '-w 0 new-tab viewer.exe --list {marked-list}'\n",
            ),
            4,
        ),
        (with_program("wt", "args = '{marked}'\n"), 4),
        (with_program("wt", "args = '-d {folder}'\n"), 4),
        (
            with_program("wt.exe", "args = '--startingDirectory={folder}'\n"),
            4,
        ),
    ];
    for (text, line) in errors {
        let error = only_error(&text);
        assert_eq!(error.line, line, "{text}");
        assert!(error.message.contains("start-in"), "{error}");
        assert!(error.message.contains("wt -d ."), "{error}");
    }

    assert_eq!(
        loaded(&with_program("wt", "args = '-d .'\n")).len(),
        1,
        "a literal Terminal command is allowed"
    );
}

#[test]
fn a_shortcut_program_with_a_placeholder_is_a_load_error() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let program = root.path().join("terminal.lnk");
    std::fs::write(&program, b"").expect("a shortcut");
    let text = with_program(&program.to_string_lossy(), "args = '{file}'\n");
    let error = only_error_with(&text, &search());
    assert_eq!(error.line, 3);
    assert!(error.message.contains("cannot be verified"), "{error}");
}

#[cfg(unix)]
#[test]
fn a_linked_program_with_a_placeholder_is_a_load_error() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let target = root.path().join("tool");
    std::fs::write(&target, b"").expect("a program");
    let link = root.path().join("terminal");
    std::os::unix::fs::symlink(&target, &link).expect("a program link");
    let text = with_program(&link.to_string_lossy(), "args = '{file}'\n");
    let error = only_error_with(&text, &search());
    assert_eq!(error.line, 3);
    assert!(error.message.contains("cannot be verified"), "{error}");
}

#[test]
fn an_unresolved_program_with_a_placeholder_still_loads() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let blocker = root.path().join("not-a-folder");
    std::fs::write(&blocker, b"x").expect("a file");
    let programs = [
        "missing-tool".to_owned(),
        blocker.join("terminal").to_string_lossy().into_owned(),
    ];
    for program in programs {
        let text = with_program(&program, "args = '{file}'\n");
        let commands = parse(&text, &search()).expect("a missing program loads");
        assert_eq!(commands.len(), 1, "{program}");
    }
}

#[cfg(unix)]
#[test]
fn a_dangling_link_with_a_placeholder_still_loads() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let link = root.path().join("terminal");
    std::os::unix::fs::symlink(root.path().join("missing"), &link).expect("a dangling link");
    let text = with_program(&link.to_string_lossy(), "args = '{file}'\n");
    assert_eq!(parse(&text, &search()).expect("it loads").len(), 1);
}

#[test]
fn a_program_that_resolves_to_a_shell_is_a_load_error() {
    let root = tempfile::tempdir().expect("a temporary folder");
    std::fs::write(root.path().join("code.cmd"), b"").expect("a batch file");
    let search = Search {
        path: root.path().to_string_lossy().into_owned(),
        extensions: ".cmd".to_owned(),
    };
    let text = with_program("code", "args = '{file}'\nstart-in = 'C:\\work'\n");
    let error = only_error_with(&text, &search);
    assert_eq!(error.line, 4);
    assert!(error.message.contains("shell"), "{error}");
    let text = with_program("code", "args = '--new'\nstart-in = 'C:\\work'\n");
    assert_eq!(parse(&text, &search).expect("no placeholder").len(), 1);
}

#[test]
fn the_default_start_folder_is_named_in_the_shell_error() {
    let error = only_error(&with_program("cmd", ""));
    assert_eq!(error.line, 1);
    assert!(error.message.contains("start-in"), "{error}");
    assert!(error.message.contains("default"), "{error}");
}

#[test]
fn a_probe_error_with_a_placeholder_is_a_load_error() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let program = root.path().join("terminal");
    std::fs::write(&program, b"").expect("a program");
    let text = with_program(&program.to_string_lossy(), "args = '{file}'\n");
    let error = with_probe(FileKind::Uncertain, || only_error_with(&text, &search()));
    assert_eq!(error.line, 3);
    assert!(error.message.contains("cannot be verified"), "{error}");
}

#[cfg(unix)]
#[test]
fn a_bare_name_found_on_path_that_is_a_link_is_a_load_error() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).expect("a bin folder");
    let target = root.path().join("tool");
    std::fs::write(&target, b"").expect("a program");
    std::os::unix::fs::symlink(&target, bin.join("terminal")).expect("a program link");
    let text = with_program("terminal", "args = '{file}'\n");
    let error = only_error_with(&text, &search_in(&bin));
    assert_eq!(error.line, 3);
    assert!(error.message.contains("cannot be verified"), "{error}");
}

#[test]
fn a_shell_command_without_placeholders_is_allowed() {
    let text = "[[command]]\nname = \"build\"\nprogram = 'cmd'\n\
                args = '/c build.cmd'\nstart-in = 'C:\\work'\n";
    let commands = loaded(text);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "cmd");
}

#[test]
fn start_in_allows_only_folders() {
    let error = only_error(&with_program("tool.exe", "start-in = '{file}'\n"));
    assert_eq!(error.line, 4);
    assert!(error.message.contains("start-in"), "{error}");
    let command = &loaded(&with_program(
        "tool.exe",
        "start-in = '{other-folder}\\x'\n",
    ))[0];
    assert_eq!(
        placeholders(&command.start_in[0]),
        [Placeholder::OtherFolder]
    );
    assert_eq!(literal(&command.start_in[0]), "{other-folder}\\x");
}

#[test]
fn braces_in_the_program_are_literal() {
    let command = &loaded(&with_program("tool{{x}}.exe", ""))[0];
    assert_eq!(command.program, "tool{x}.exe");
}

#[test]
fn broken_toml_is_a_lined_error() {
    let error = only_error("[[command]\n");
    assert_eq!(error.line, 1);
    assert!(!error.message.is_empty());
}

#[test]
fn unknown_fields_and_missing_fields_are_errors() {
    assert!(parse_without_identity(&command("colour = \"red\"\n")).is_err());
    assert!(parse_without_identity("[[command]]\nname = \"x\"\n").is_err());
}

#[test]
fn crlf_and_bom_files_parse() {
    let text = "\u{feff}[[command]]\r\nname = \"x\"\r\nprogram = \"tool.exe\"\r\n";
    assert_eq!(loaded(text)[0].name, "x");
}
