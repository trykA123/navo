use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use navo_ui::git::GitAction;

use super::start::{Ports, start, windows_flags};
use super::{Arg, Launch, LaunchError, Process, plan};
use crate::git::tools::{DetectedTool, ToolKind};

const REPO: &str = r"C:\Work Space\répo ü";

fn tool(kind: ToolKind) -> DetectedTool {
    DetectedTool {
        kind,
        label: kind.label(),
        exe: PathBuf::from(r"C:\Tools\tool.exe"),
    }
}

fn custom() -> DetectedTool {
    DetectedTool {
        kind: ToolKind::Custom,
        label: ToolKind::Custom.label(),
        exe: PathBuf::new(),
    }
}

fn process(launch: Launch) -> Process {
    match launch {
        Launch::Process(process) => process,
        other => panic!("not a process: {other:?}"),
    }
}

fn plain(args: &[&str]) -> Vec<Arg> {
    args.iter()
        .map(|arg| Arg::Plain((*arg).to_owned()))
        .collect()
}

fn custom_plan(template: &str, repo: &str) -> Result<Launch, LaunchError> {
    plan(&custom(), GitAction::Log, repo, Some(template))
}

#[test]
fn tortoisegit_commit_gets_one_raw_path_argument() {
    let launch = plan(
        &tool(ToolKind::TortoiseGit),
        GitAction::Commit,
        r"C:\a b\",
        None,
    );
    let process = process(launch.unwrap());
    assert_eq!(process.program, PathBuf::from(r"C:\Tools\tool.exe"));
    assert_eq!(
        process.args,
        [
            Arg::Plain("/command:commit".to_owned()),
            Arg::Raw(r#"/path:"C:\a b""#.to_owned()),
        ]
    );
    assert!(!process.console);
}

#[test]
fn tortoisegit_maps_every_action() {
    for (action, command) in [
        (GitAction::Commit, "/command:commit"),
        (GitAction::Pull, "/command:pull"),
        (GitAction::Push, "/command:push"),
        (GitAction::Log, "/command:log"),
        (GitAction::Diff, "/command:repostatus"),
    ] {
        let launch = plan(&tool(ToolKind::TortoiseGit), action, REPO, None);
        let args = process(launch.unwrap()).args;
        assert_eq!(args[0], Arg::Plain(command.to_owned()));
        assert_eq!(args[1], Arg::Raw(format!("/path:\"{REPO}\"")));
    }
}

#[test]
fn tortoisegit_paths_never_end_in_a_backslash_before_the_quote() {
    for (repo, shown) in [
        (r"C:\a b\\", r"C:\a b"),
        ("C:/a b/", "C:/a b"),
        (r"C:\", r"C:\."),
        (r"\\srv\share\repo\", r"\\srv\share\repo"),
    ] {
        let launch = plan(&tool(ToolKind::TortoiseGit), GitAction::Log, repo, None);
        let args = process(launch.unwrap()).args;
        assert_eq!(args[1], Arg::Raw(format!("/path:\"{shown}\"")));
    }
}

#[test]
fn unusable_repository_paths_are_refused_for_every_tool() {
    for repo in [
        "",
        "C:\\a\"b",
        "C:\\a\nb",
        "C:\\a\u{7f}b",
        "-rf",
        "C:\\a\0b",
    ] {
        for kind in [ToolKind::TortoiseGit, ToolKind::Fork, ToolKind::GitCli] {
            let result = plan(&tool(kind), GitAction::Log, repo, None);
            assert_eq!(result, Err(LaunchError::BadPath), "{repo:?} {kind:?}");
        }
        assert_eq!(custom_plan("tool {path}", repo), Err(LaunchError::BadPath));
    }
}

#[test]
fn open_only_tools_get_the_repository_as_their_open_argument() {
    for (kind, args) in [
        (ToolKind::Fork, plain(&[REPO])),
        (ToolKind::GitHubDesktop, plain(&["--cli-open", REPO])),
        (ToolKind::SourceTree, plain(&["-f", REPO])),
        (ToolKind::GitKraken, plain(&[REPO])),
        (ToolKind::SublimeMerge, plain(&[REPO])),
        (ToolKind::VsCode, plain(&[REPO])),
    ] {
        let process = process(plan(&tool(kind), GitAction::Open, REPO, None).unwrap());
        assert_eq!(process.args, args, "{kind:?}");
        assert_eq!(process.program, PathBuf::from(r"C:\Tools\tool.exe"));
    }
}

#[test]
fn console_programs_hide_their_window_and_gui_tools_do_not() {
    for (kind, console) in [
        (ToolKind::Fork, false),
        (ToolKind::GitHubDesktop, false),
        (ToolKind::SublimeMerge, true),
        (ToolKind::VsCode, true),
    ] {
        let process = process(plan(&tool(kind), GitAction::Open, REPO, None).unwrap());
        assert_eq!(process.console, console, "{kind:?}");
    }
    assert_eq!(windows_flags(true), 0x0800_0000 | 0x200);
    assert_eq!(windows_flags(false), 0x200);
}

#[test]
fn a_tool_only_does_what_its_capabilities_say() {
    let refused = [
        (ToolKind::Fork, GitAction::Commit),
        (ToolKind::VsCode, GitAction::Diff),
        (ToolKind::TortoiseGit, GitAction::Open),
        (ToolKind::TortoiseGit, GitAction::ChooseTool),
        (ToolKind::GitCli, GitAction::Pull),
        (ToolKind::GitCli, GitAction::Push),
        (ToolKind::GitCli, GitAction::Diff),
        (ToolKind::Custom, GitAction::Open),
    ];
    for (kind, action) in refused {
        let result = plan(&tool(kind), action, REPO, Some("tool {path}"));
        assert_eq!(result, Err(LaunchError::Unsupported(kind.label())));
    }
}

#[test]
fn git_cli_commit_and_log_open_the_terminal_in_the_repository() {
    for action in [GitAction::Commit, GitAction::Log] {
        let launch = plan(&tool(ToolKind::GitCli), action, REPO, None).unwrap();
        assert_eq!(
            launch,
            Launch::Terminal {
                repo: REPO.to_owned()
            }
        );
    }
}

#[test]
fn custom_templates_expand_path_and_action_as_whole_arguments() {
    let template = r#""C:\My Tool\tool.exe" --do {action} --repo {path} --tag={action}"#;
    let process = process(custom_plan(template, REPO).unwrap());
    assert_eq!(process.program, PathBuf::from(r"C:\My Tool\tool.exe"));
    assert_eq!(
        process.args,
        plain(&["--do", "log", "--repo", REPO, "--tag=log"])
    );
    assert!(process.console);
}

#[test]
fn custom_action_names_follow_the_menu() {
    for (action, name) in [
        (GitAction::Commit, "commit"),
        (GitAction::Pull, "pull"),
        (GitAction::Push, "push"),
        (GitAction::Log, "log"),
        (GitAction::Diff, "diff"),
    ] {
        let launch = plan(&custom(), action, REPO, Some("tool {action} {path}"));
        assert_eq!(
            process(launch.unwrap()).args[0],
            Arg::Plain(name.to_owned())
        );
    }
}

#[test]
fn a_repository_path_with_shell_characters_stays_one_argument() {
    let repo = r"C:\x&calc|^%$;` (1)\r";
    let process = process(custom_plan("tool {path}", repo).unwrap());
    assert_eq!(process.args, plain(&[repo]));
}

#[test]
fn custom_templates_refuse_unsafe_placement() {
    for template in [
        "tool --repo={path}",
        "tool \"cd {path}\"",
        "cmd.exe /c tool {path}",
        "x.bat {path}",
        "sh -c tool {path}",
        "wt.exe cmd.exe /K {path}",
    ] {
        assert_eq!(
            custom_plan(template, REPO),
            Err(LaunchError::PathPlacement),
            "{template}"
        );
    }
    for template in ["{path} --x", "{action} {path}"] {
        assert_eq!(
            custom_plan(template, REPO),
            Err(LaunchError::ProgramPlaceholder)
        );
    }
}

#[test]
fn a_custom_template_without_path_or_program_is_refused() {
    assert_eq!(
        custom_plan("tool {action}", REPO),
        Err(LaunchError::MissingPath)
    );
    assert_eq!(
        custom_plan("tool --flag", REPO),
        Err(LaunchError::MissingPath)
    );
    assert_eq!(custom_plan("   ", REPO), Err(LaunchError::EmptyCommand));
    let none = plan(&custom(), GitAction::Log, REPO, None);
    assert_eq!(none, Err(LaunchError::EmptyCommand));
    assert!(matches!(
        custom_plan("tool \"{path}", REPO),
        Err(LaunchError::Command(_))
    ));
}

#[test]
fn custom_windows_terminal_escapes_semicolons() {
    let process = process(custom_plan("wt.exe -d {path}", r"C:\a;b").unwrap());
    assert_eq!(process.args, plain(&["-d", r"C:\a\;b"]));
}

fn ports() -> (Ports, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let terminal = Arc::clone(&seen);
    let url = Arc::clone(&seen);
    let ports = Ports {
        terminal: Arc::new(move |location| {
            terminal
                .lock()
                .unwrap()
                .push(format!("terminal {}", location.as_str()));
            Ok(())
        }),
        url: Arc::new(move |text| url.lock().unwrap().push(format!("url {text}"))),
    };
    (ports, seen)
}

fn finished() -> (Box<dyn FnOnce() + Send>, mpsc::Receiver<()>) {
    let (sender, receiver) = mpsc::channel();
    (
        Box::new(move || {
            let _ = sender.send(());
        }),
        receiver,
    )
}

#[cfg(unix)]
#[test]
fn a_process_launch_calls_on_exit_when_the_process_ends() {
    let (ports, _) = ports();
    let (on_exit, done) = finished();
    let launch = Launch::Process(Process {
        program: PathBuf::from("true"),
        args: plain(&["x"]),
        console: false,
    });
    start(launch, &ports, on_exit).unwrap();
    assert!(done.recv_timeout(Duration::from_secs(5)).is_ok());
}

#[test]
fn a_missing_program_reports_a_spawn_error_and_never_calls_on_exit() {
    let (ports, _) = ports();
    let (on_exit, done) = finished();
    let launch = Launch::Process(Process {
        program: PathBuf::from("/nonexistent/navo-git-tool"),
        args: Vec::new(),
        console: true,
    });
    assert!(matches!(
        start(launch, &ports, on_exit),
        Err(LaunchError::Spawn(_))
    ));
    assert!(done.recv_timeout(Duration::from_millis(100)).is_err());
}

#[test]
fn terminal_and_url_launches_use_their_ports() {
    let (ports, seen) = ports();
    let (on_exit, _done) = finished();
    start(
        Launch::Terminal {
            repo: "/work/repo".to_owned(),
        },
        &ports,
        on_exit,
    )
    .unwrap();
    let (on_exit, _done) = finished();
    start(Launch::Url("tool://open".to_owned()), &ports, on_exit).unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        ["terminal /work/repo", "url tool://open"]
    );
}
