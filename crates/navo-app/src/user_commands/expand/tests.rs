use std::path::{Path, PathBuf};

use navo_core::Location;

use crate::editor::Search;
use crate::user_commands::model::{self, Console, Part, Placeholder, Token, UserCommand};

use super::{Invocation, LIST_PATH_MARKER, MAX_COMMAND_LINE, Refusal, plan};

struct Rig {
    root: tempfile::TempDir,
    folder: PathBuf,
}

impl Rig {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("a temporary folder");
        let folder = root.path().join("work");
        std::fs::create_dir_all(&folder).expect("the work folder");
        Self { root, folder }
    }

    fn tool(&self, name: &str) -> PathBuf {
        let path = self.root.path().join(tool_file(name));
        std::fs::write(&path, b"").expect("a tool");
        path
    }

    fn executable(&self, name: &str) -> PathBuf {
        let path = self.root.path().join(name);
        std::fs::write(&path, b"").expect("a program");
        path
    }

    fn search(&self) -> Search {
        Search {
            path: self.root.path().to_string_lossy().into_owned(),
            extensions: if cfg!(windows) {
                ".EXE".to_owned()
            } else {
                String::new()
            },
        }
    }

    fn invocation(&self) -> Invocation {
        Invocation {
            file: Some(loc(self.folder.join("a b.txt"))),
            marked: Vec::new(),
            folder: Some(loc(&self.folder)),
            other_folder: Some(loc(self.root.path().join("other"))),
            other_file: Some(loc(self.root.path().join("other x.txt"))),
        }
    }

    fn start(&self) -> String {
        self.folder.to_string_lossy().into_owned()
    }
}

fn tool_file(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn loc(path: impl AsRef<Path>) -> Location {
    Location::new(path.as_ref().to_string_lossy()).expect("a location")
}

fn parsed(program: &str, args: &str, start_in: Option<&str>) -> UserCommand {
    let mut text =
        format!("[[command]]\nname = \"test\"\nprogram = '{program}'\nargs = '{args}'\n");
    if let Some(start_in) = start_in {
        text.push_str(&format!("start-in = '{start_in}'\n"));
    }
    let mut commands = model::parse_without_identity(&text).expect("a valid command");
    assert_eq!(commands.len(), 1);
    commands.remove(0)
}

fn token(parts: Vec<Part>) -> Token {
    Token { parts }
}

fn literal(text: &str) -> Token {
    token(vec![Part::Literal(text.to_owned())])
}

fn placeholder(placeholder: Placeholder) -> Token {
    token(vec![Part::Placeholder(placeholder)])
}

fn terminal(rig: &Rig, args: Vec<Token>) -> UserCommand {
    let start = rig.start();
    let mut command = parsed("wt", "", Some(&start));
    command.args = args;
    command
}

fn refused_terminal(rig: &Rig, invocation: &Invocation, args: Vec<Token>) {
    let command = terminal(rig, args);
    assert!(
        matches!(
            plan(&command, invocation, &rig.search()),
            Err(Refusal::Terminal(_))
        ),
        "{:?}",
        command.args
    );
}

#[test]
fn every_value_stays_one_argument() {
    let rig = Rig::new();
    rig.tool("tool");
    let args = "\"a b\" \"say \"\"hi\"\"\" a&b 100% C:\\dir\\";
    let command = parsed("tool", args, None);
    let planned = plan(&command, &rig.invocation(), &rig.search()).expect("a plan");
    assert_eq!(
        planned.args,
        ["a b", "say \"hi\"", "a&b", "100%", "C:\\dir\\"]
    );
}

#[test]
fn a_placeholder_value_stays_one_argument() {
    let rig = Rig::new();
    rig.tool("tool");
    let file = rig.folder.join("a & b%c\"d.txt");
    let mut invocation = rig.invocation();
    invocation.file = Some(loc(&file));
    let command = parsed("tool", "{file}", None);
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(planned.args, [file.to_string_lossy().into_owned()]);
}

#[test]
fn placeholders_expand_to_their_values() {
    let rig = Rig::new();
    rig.tool("tool");
    let file = rig.folder.join("a.b c.txt");
    let other_folder = rig.root.path().join("other");
    std::fs::create_dir_all(&other_folder).expect("the other folder");
    let other_file = rig.root.path().join("other x.txt");
    let mut invocation = rig.invocation();
    invocation.file = Some(loc(&file));
    invocation.other_folder = Some(loc(&other_folder));
    invocation.other_file = Some(loc(&other_file));
    let command = parsed(
        "tool",
        "{file} {name} {stem} {ext} {folder} {other-folder} {other-file}",
        None,
    );
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(
        planned.args,
        [
            file.to_string_lossy().into_owned(),
            "a.b c.txt".to_owned(),
            "a.b c".to_owned(),
            "txt".to_owned(),
            rig.folder.to_string_lossy().into_owned(),
            other_folder.to_string_lossy().into_owned(),
            other_file.to_string_lossy().into_owned(),
        ]
    );
}

#[test]
fn a_name_without_an_extension_has_an_empty_extension() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut invocation = rig.invocation();
    invocation.file = Some(loc(rig.folder.join("README")));
    let command = parsed("tool", "{name} {stem} {ext}", None);
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(planned.args, ["README", "README", ""]);
}

#[test]
fn marked_is_each_item_and_falls_back_to_the_cursor() {
    let rig = Rig::new();
    rig.tool("tool");
    let first = loc(rig.folder.join("first.txt"));
    let second = loc(rig.folder.join("second.txt"));
    let mut invocation = rig.invocation();
    invocation.marked = vec![first.clone(), second.clone()];
    let command = parsed("tool", "{marked}", None);
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(planned.args, [first.as_str(), second.as_str()]);

    invocation.marked.clear();
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(
        planned.args,
        [invocation.file.as_ref().expect("a file").as_str()]
    );
}

#[test]
fn marked_list_gives_a_list_and_its_argument() {
    let rig = Rig::new();
    rig.tool("tool");
    let first = loc(rig.folder.join("first.txt"));
    let second = loc(rig.folder.join("second.txt"));
    let mut invocation = rig.invocation();
    invocation.marked = vec![first.clone(), second.clone()];
    let command = parsed("tool", "--list={marked-list}", None);
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(planned.args, [format!("--list={LIST_PATH_MARKER}")]);
    assert_eq!(
        planned.list,
        Some(vec![first.as_str().to_owned(), second.as_str().to_owned()])
    );

    invocation.marked.clear();
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(
        planned.list,
        Some(vec![
            invocation
                .file
                .as_ref()
                .expect("a file")
                .as_str()
                .to_owned()
        ])
    );
}

#[test]
fn a_command_line_at_the_limit_is_allowed_and_over_it_is_refused() {
    let rig = Rig::new();
    let tool = rig.tool("tool");
    let units = super::serialized_units(&tool.to_string_lossy());
    let at = MAX_COMMAND_LINE - units - 1;
    let command = parsed("tool", &"x".repeat(at), None);
    let planned = plan(&command, &rig.invocation(), &rig.search()).expect("at the limit");
    assert_eq!(planned.args[0].len(), at);

    let over = parsed("tool", &"x".repeat(at + 1), None);
    assert_eq!(
        plan(&over, &rig.invocation(), &rig.search()),
        Err(Refusal::TooLong)
    );
    assert!(Refusal::TooLong.to_string().contains("{marked-list}"));
}

#[test]
fn a_resolved_batch_program_is_a_shell() {
    let rig = Rig::new();
    rig.executable("build.cmd");
    let mut search = rig.search();
    search.extensions = ".CMD".to_owned();
    let command = parsed("build", "{file}", None);
    assert_eq!(
        plan(&command, &rig.invocation(), &search),
        Err(Refusal::ShellPlaceholder)
    );
}

#[test]
fn a_short_shell_executable_name_is_a_shell() {
    let short = Path::new(r"C:\Windows\System32\POWERS~1.EXE");
    assert_eq!(
        super::is_shell_executable(short, |_| {
            Some(PathBuf::from(
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            ))
        }),
        Some(true)
    );
}

#[test]
fn a_short_name_without_a_long_name_is_uncertain() {
    let short = Path::new(r"C:\Windows\System32\POWERS~1.EXE");
    assert_eq!(super::is_shell_executable(short, |_| None), None);
}

#[test]
fn a_terminal_command_with_a_placeholder_is_refused() {
    let rig = Rig::new();
    rig.tool("wt");
    let invocation = rig.invocation();
    refused_terminal(
        &rig,
        &invocation,
        vec![
            literal(r"C:\Windows\System32\WindowsPowerShell\v1.0\POWERS~1.EXE"),
            literal("-Command"),
            literal("echo"),
            literal("-d"),
            placeholder(Placeholder::Stem),
        ],
    );
    refused_terminal(
        &rig,
        &invocation,
        vec![
            literal("new-tab"),
            literal("-p"),
            literal("Windows PowerShell"),
            literal("--appendCommandLine"),
            literal("echo"),
            literal("-d"),
            placeholder(Placeholder::Stem),
        ],
    );
    refused_terminal(
        &rig,
        &invocation,
        vec![token(vec![
            Part::Literal("-d=.;".to_owned()),
            Part::Placeholder(Placeholder::Stem),
        ])],
    );
    refused_terminal(
        &rig,
        &invocation,
        vec![
            literal("-w"),
            literal("0"),
            literal("new-tab"),
            literal("viewer.exe"),
            literal("--list"),
            placeholder(Placeholder::MarkedList),
        ],
    );
}

#[test]
fn a_shortcut_program_with_placeholders_is_unknown() {
    let rig = Rig::new();
    rig.executable("terminal.lnk");
    let start = rig.start();
    let mut command = parsed("terminal.lnk", "", Some(&start));
    command.args = vec![placeholder(Placeholder::File)];
    assert_eq!(
        plan(&command, &rig.invocation(), &rig.search()),
        Err(Refusal::UnknownProgram)
    );
}

#[cfg(unix)]
#[test]
fn a_linked_program_with_placeholders_is_unknown() {
    let rig = Rig::new();
    let target = rig.tool("tool");
    let link = rig.root.path().join("terminal");
    std::os::unix::fs::symlink(&target, &link).expect("a program link");
    let start = rig.start();
    let mut command = parsed(&link.to_string_lossy(), "", Some(&start));
    command.args = vec![placeholder(Placeholder::File)];
    assert_eq!(
        plan(&command, &rig.invocation(), &rig.search()),
        Err(Refusal::UnknownProgram)
    );
}

#[test]
fn a_probe_error_at_runtime_is_unknown() {
    let rig = Rig::new();
    rig.tool("tool");
    let command = parsed("tool", "{file}", None);
    let result =
        super::super::program::with_probe(super::super::program::FileKind::Uncertain, || {
            plan(&command, &rig.invocation(), &rig.search())
        });
    assert_eq!(result, Err(Refusal::UnknownProgram));
}

#[test]
fn a_faked_reparse_probe_is_uncertain() {
    use super::super::program::{FileKind, uncertain};

    let path = Path::new(r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\terminal.exe");
    assert!(uncertain(path, |_| FileKind::Uncertain));
    assert!(!uncertain(path, |_| FileKind::Real));
}

#[test]
fn a_terminal_start_directory_placeholder_is_refused() {
    let rig = Rig::new();
    rig.tool("wt");
    let invocation = rig.invocation();
    refused_terminal(
        &rig,
        &invocation,
        vec![literal("-d"), placeholder(Placeholder::Folder)],
    );
    refused_terminal(
        &rig,
        &invocation,
        vec![token(vec![
            Part::Literal("--startingDirectory=".to_owned()),
            Part::Placeholder(Placeholder::Folder),
        ])],
    );
    refused_terminal(&rig, &invocation, vec![placeholder(Placeholder::File)]);
}

#[test]
fn a_terminal_command_without_a_placeholder_is_allowed() {
    let rig = Rig::new();
    rig.tool("wt");
    let invocation = rig.invocation();
    for (args, expected) in [
        ("-d .", vec!["-d", "."]),
        ("--startingDirectory .", vec!["--startingDirectory", "."]),
    ] {
        let command = parsed("wt", args, None);
        let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
        assert_eq!(
            planned.args,
            expected.into_iter().map(str::to_owned).collect::<Vec<_>>()
        );
    }
}

#[test]
fn an_expanded_shell_word_is_refused() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut invocation = rig.invocation();
    invocation.file = Some(loc(rig.folder.join("echo SAFE&calc.exe")));
    let command = parsed("tool", "cmd.{ext} /c {name}", None);
    assert_eq!(
        plan(&command, &invocation, &rig.search()),
        Err(Refusal::ShellPlaceholder)
    );
}

#[test]
fn a_placeholder_alone_may_name_a_shell_script() {
    let rig = Rig::new();
    rig.tool("tool");
    for name in ["build.bat", "x.ps1", "run.cmd"] {
        let file = rig.folder.join(name);
        let mut invocation = rig.invocation();
        invocation.file = Some(loc(&file));
        let command = parsed("tool", "{file} {name}", None);
        let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
        assert_eq!(
            planned.args,
            [file.to_string_lossy().into_owned(), name.to_owned()]
        );
    }
}

#[test]
fn a_folder_ending_in_sh_is_allowed_and_marked_paths_are_not_checked() {
    let rig = Rig::new();
    rig.tool("tool");
    let folder = rig.root.path().join("work").join("bash");
    std::fs::create_dir_all(&folder).expect("the folder");
    let marked = rig.folder.join("go.bat");
    let mut invocation = rig.invocation();
    invocation.folder = Some(loc(&folder));
    invocation.marked = vec![loc(&marked)];
    let command = parsed("tool", "{folder} {marked}", Some(&folder.to_string_lossy()));
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(
        planned.args,
        [
            folder.to_string_lossy().into_owned(),
            marked.to_string_lossy().into_owned()
        ]
    );
}

#[test]
fn a_dash_name_gets_a_dot_slash_prefix() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut invocation = rig.invocation();
    invocation.file = Some(loc(rig.folder.join("--output=x")));
    let command = parsed("tool", "{name} {stem} x{name} {file}", None);
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    let dot = if cfg!(windows) { ".\\" } else { "./" };
    assert_eq!(planned.args[0], format!("{dot}--output=x"));
    assert_eq!(planned.args[1], format!("{dot}--output=x"));
    assert_eq!(planned.args[2], "x--output=x");
    assert_eq!(
        planned.args[3],
        rig.folder.join("--output=x").to_string_lossy()
    );
}

#[test]
fn a_dash_extension_gets_a_dot_slash_prefix() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut invocation = rig.invocation();
    invocation.file = Some(loc(rig.folder.join("a.-v")));
    let command = parsed("tool", "{ext}", None);
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    let dot = if cfg!(windows) { ".\\" } else { "./" };
    assert_eq!(planned.args, [format!("{dot}-v")]);
}

#[test]
fn a_missing_program_loads_with_the_others_and_is_refused_when_run() {
    let rig = Rig::new();
    rig.tool("tool");
    let text = "[[command]]\nname = \"gone\"\nprogram = 'gone-tool'\nargs = '{file}'\n\n\
                [[command]]\nname = \"here\"\nprogram = 'tool'\nargs = '{file}'\n";
    let mut commands = model::parse(text, &rig.search()).expect("both load");
    assert_eq!(commands.len(), 2);
    let here = commands.remove(1);
    let gone = commands.remove(0);
    assert_eq!(
        plan(&gone, &rig.invocation(), &rig.search()),
        Err(Refusal::ProgramNotFound)
    );
    assert!(plan(&here, &rig.invocation(), &rig.search()).is_ok());
}

#[test]
fn eleven_thousand_empty_arguments_are_too_long() {
    let rig = Rig::new();
    rig.tool("tool");
    let empty = vec!["\"\""; 11_000].join(" ");
    let command = parsed("tool", &empty, None);
    assert_eq!(
        plan(&command, &rig.invocation(), &rig.search()),
        Err(Refusal::TooLong)
    );
}

#[test]
fn a_command_without_placeholders_still_needs_a_cursor() {
    let rig = Rig::new();
    rig.executable("tool.exe");
    let start = rig.start();
    let command = parsed("tool.exe", "", Some(&start));
    let mut invocation = rig.invocation();
    invocation.file = None;
    assert_eq!(
        plan(&command, &invocation, &rig.search()),
        Err(Refusal::NoCursor)
    );
    invocation.file = Some(loc(r"C:\x\.."));
    assert_eq!(
        plan(&command, &invocation, &rig.search()),
        Err(Refusal::DotDot)
    );
}

type Case = (&'static str, UserCommand, Invocation, Refusal);

fn check_cases(rig: &Rig, cases: Vec<Case>) {
    for (name, command, invocation, expected) in cases {
        assert_eq!(
            plan(&command, &invocation, &rig.search()),
            Err(expected),
            "{name}"
        );
    }
}

#[test]
fn cursor_refusals_are_named() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut cases: Vec<Case> = Vec::new();

    let mut invocation = rig.invocation();
    invocation.file = None;
    cases.push((
        "no cursor",
        parsed("tool", "{file}", None),
        invocation,
        Refusal::NoCursor,
    ));

    let mut invocation = rig.invocation();
    invocation.file = Some(loc(r"C:\x\.."));
    cases.push((
        "dot dot",
        parsed("tool", "{file}", None),
        invocation,
        Refusal::DotDot,
    ));

    let mut invocation = rig.invocation();
    invocation.file = Some(Location::new("..").expect("a relative dot dot"));
    cases.push((
        "relative dot dot",
        parsed("tool", "{file}", None),
        invocation,
        Refusal::DotDot,
    ));

    let mut invocation = rig.invocation();
    invocation.file = Some(Location::new("archive://a.zip").expect("an archive"));
    cases.push((
        "non local",
        parsed("tool", "{file}", None),
        invocation,
        Refusal::NonLocal,
    ));

    check_cases(&rig, cases);
}

#[test]
fn marked_refusals_are_named() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut invocation = rig.invocation();
    invocation.marked = vec![Location::new("archive://a.zip").expect("an archive")];
    check_cases(
        &rig,
        vec![(
            "non local marked",
            parsed("tool", "{marked}", None),
            invocation,
            Refusal::NonLocal,
        )],
    );
}

#[test]
fn path_refusals_are_named() {
    let rig = Rig::new();
    rig.tool("tool");
    let missing = rig.folder.join("missing").to_string_lossy().into_owned();
    check_cases(
        &rig,
        vec![
            (
                "missing start",
                parsed("tool", "{file}", Some(&missing)),
                rig.invocation(),
                Refusal::MissingStart,
            ),
            (
                "program",
                parsed("missing-tool", "", None),
                rig.invocation(),
                Refusal::ProgramNotFound,
            ),
        ],
    );
}

#[test]
fn other_pane_refusals_are_named() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut cases: Vec<Case> = Vec::new();

    let mut invocation = rig.invocation();
    invocation.other_file = None;
    cases.push((
        "other file",
        parsed("tool", "{other-file}", None),
        invocation,
        Refusal::NoOtherFile,
    ));

    let mut invocation = rig.invocation();
    invocation.other_folder = None;
    cases.push((
        "other folder",
        parsed("tool", "{other-folder}", None),
        invocation,
        Refusal::NoOtherFolder,
    ));

    let mut invocation = rig.invocation();
    invocation.folder = None;
    cases.push((
        "folder",
        parsed("tool", "{folder}", None),
        invocation,
        Refusal::NoFolder,
    ));

    check_cases(&rig, cases);
}

#[test]
fn a_shell_with_a_placeholder_is_refused() {
    let rig = Rig::new();
    rig.tool("tool");
    let start = rig.start();

    let mut command = parsed("tool", "", Some(&start));
    command.program = "\"cmd\"".to_owned();
    command.args = vec![Token {
        parts: vec![Part::Placeholder(Placeholder::File)],
    }];
    assert_eq!(
        plan(&command, &rig.invocation(), &rig.search()),
        Err(Refusal::ShellPlaceholder)
    );

    let mut command = parsed("tool", "", Some(&start));
    command.args = vec![
        Token {
            parts: vec![Part::Literal("run.ps1".to_owned())],
        },
        Token {
            parts: vec![Part::Placeholder(Placeholder::File)],
        },
    ];
    assert_eq!(
        plan(&command, &rig.invocation(), &rig.search()),
        Err(Refusal::ShellPlaceholder)
    );

    let mut command = parsed("tool", "", Some(&start));
    command.args = vec![
        Token {
            parts: vec![Part::Literal("run.ps1.".to_owned())],
        },
        Token {
            parts: vec![Part::Placeholder(Placeholder::File)],
        },
    ];
    assert_eq!(
        plan(&command, &rig.invocation(), &rig.search()),
        Err(Refusal::ShellPlaceholder)
    );
}

#[test]
fn a_shell_without_placeholders_is_allowed() {
    let rig = Rig::new();
    let cmd = rig.tool("cmd");
    let start = rig.start();
    let mut command = parsed("tool", "/c exit 0", Some(&start));
    command.program = "cmd".to_owned();
    let planned = plan(&command, &rig.invocation(), &rig.search()).expect("a plan");
    assert_eq!(planned.program, cmd);
}

#[test]
fn marked_must_be_a_whole_argument() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut command = parsed("tool", "", None);
    command.args = vec![Token {
        parts: vec![
            Part::Literal("x".to_owned()),
            Part::Placeholder(Placeholder::Marked),
        ],
    }];
    assert_eq!(
        plan(&command, &rig.invocation(), &rig.search()),
        Err(Refusal::MarkedNotWhole)
    );
}

#[test]
fn start_in_must_be_one_value() {
    let rig = Rig::new();
    rig.tool("tool");
    let mut command = parsed("tool", "", None);
    command.start_in = vec![Token {
        parts: vec![Part::Placeholder(Placeholder::File)],
    }];
    assert_eq!(
        plan(&command, &rig.invocation(), &rig.search()),
        Err(Refusal::BadStartIn)
    );
}

#[test]
fn the_program_is_resolved_and_the_console_is_kept() {
    let rig = Rig::new();
    let tool = rig.tool("tool");
    let mut command = parsed("tool", "", None);
    command.console = Console::Hide;
    let planned = plan(&command, &rig.invocation(), &rig.search()).expect("a plan");
    assert_eq!(planned.program, tool);
    assert_eq!(planned.start_in, rig.folder);
    assert_eq!(planned.console, Console::Hide);
    assert_eq!(planned.list, None);
}

#[test]
fn the_start_folder_expands_and_must_exist() {
    let rig = Rig::new();
    rig.tool("tool");
    let other = rig.root.path().join("other");
    std::fs::create_dir_all(&other).expect("the other folder");
    let mut invocation = rig.invocation();
    invocation.other_folder = Some(loc(&other));
    let command = parsed("tool", "", Some("{other-folder}"));
    let planned = plan(&command, &invocation, &rig.search()).expect("a plan");
    assert_eq!(planned.start_in, other);

    invocation.other_folder = None;
    assert_eq!(
        plan(&command, &invocation, &rig.search()),
        Err(Refusal::NoOtherFolder)
    );
}
