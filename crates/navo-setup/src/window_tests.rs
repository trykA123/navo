//! Window tests: the flow-to-view mapping, stale results, the Close guard and the debounce.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use gpui_kit::{AppContext, Entity, TestAppContext, WindowOptions};
use navo_ui::setup::{SetupAction, SetupCheck, SetupPage};
use navo_update::parse_version;

use crate::flow::{Completion, Event, Failure, SetupExit};
use crate::mode::{Discovery, Mode};
use crate::payload::Payload;
use crate::placement::FolderWarning;
use crate::runner::Progress;
use crate::runner_fake::FakeHost;
use crate::syntax::{Candidate, FolderProblem, parse_candidate};
use crate::window::{DEBOUNCE, SetupWindow};
use crate::window_core::{Out, Shell, initial_flow};
use crate::window_view::Facts;
use crate::window_worker::{Job, Link, Reply};

pub const ROOT: &str = r"C:\Programs\Navo";
pub const OTHER: &str = r"D:\Apps\Navo";
pub const VERSION: &str = "0.4.0";
pub const WIZARD_LOG: &str = r"C:\Temp\wizard.log";

/// Builds a validated file payload in `dir`, as the runner tests do.
pub fn payload(dir: &Path) -> Payload {
    use std::io::Write;
    let setup = dir.join("input.exe");
    std::fs::write(&setup, b"fixture Setup.exe").unwrap();
    let package = dir.join(format!("NavoApp-{VERSION}-full.nupkg"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&package).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("lib/net8/NavoApp.nuspec", options).unwrap();
    let nuspec = format!(
        "<package><metadata><id>NavoApp</id><version>{VERSION}</version></metadata></package>"
    );
    zip.write_all(nuspec.as_bytes()).unwrap();
    zip.finish().unwrap();
    Payload::from_files(&setup, &package, parse_version(VERSION).unwrap()).unwrap()
}

pub fn candidate(path: &str) -> Candidate {
    parse_candidate(Path::new(path).as_os_str()).unwrap()
}

fn facts() -> Facts {
    Facts {
        version: VERSION.into(),
        space_needed: Some(200),
        wizard_log: Some(PathBuf::from(WIZARD_LOG)),
        log_paths: vec![PathBuf::from(r"C:\Temp\velopack.log")],
    }
}

/// A fresh-install shell on Welcome, discovered through a fake host.
pub fn fresh_shell() -> Shell {
    let dir = tempfile::tempdir().unwrap();
    let mut host = FakeHost::new();
    host.discoveries.push_back(Ok(Discovery::default()));
    let flow = initial_flow(&mut host, Some(&payload(dir.path())), ROOT.into());
    Shell::new(flow, facts())
}

/// Walks a fresh shell to the Folder page with ROOT waiting for its check.
fn shell_checking_folder() -> Shell {
    let mut shell = fresh_shell();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Toggle(SetupCheck::AcceptLicence));
    shell.act(SetupAction::Next);
    shell
}

/// Walks a fresh shell to the Folder page with ROOT checked and ready.
pub fn shell_on_ready_folder() -> Shell {
    let mut shell = fresh_shell();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Toggle(SetupCheck::AcceptLicence));
    shell.act(SetupAction::Next);
    shell.reply(Reply::Checked {
        candidate: candidate(ROOT),
        result: Ok(()),
        warning: None,
        free: Some(9_000),
    });
    shell
}

pub fn exited(exit: SetupExit) -> Reply {
    Reply::Event(Event::SetupExited(exit))
}

/// An upgrade shell on Welcome for the Navo 0.3.0 installed in OTHER.
pub fn upgrade_shell() -> Shell {
    let flow = crate::flow::Flow::new(Mode::Upgrade(installed()), ROOT.into());
    Shell::new(flow, facts())
}

/// The healthy Navo 0.3.0 install in OTHER that discovery reports.
pub fn installed() -> crate::mode::Install {
    crate::mode::Install {
        root: OTHER.into(),
        identity: navo_core::FileIdentity {
            volume: 1,
            file: [2; 16],
            kind: navo_core::FileIdentityKind::Native,
        },
        safe_chain: true,
        update_exe: true,
        sq_version: crate::mode::parse_sq_version(
            "<package><metadata><id>NavoApp</id><version>0.3.0</version></metadata></package>",
        ),
        marker: None,
    }
}

/// A shell on the page that `mode` opens with, as discovery would start it.
pub fn shell_in(mode: Mode) -> Shell {
    Shell::new(crate::flow::Flow::new(mode, ROOT.into()), facts())
}

#[test]
fn the_first_pages_map_their_gates_and_primary_button() {
    let mut shell = fresh_shell();
    let welcome = shell.view();
    assert_eq!(welcome.page, SetupPage::Welcome);
    assert!(welcome.can_next() && !welcome.can_back());
    assert_eq!(welcome.primary, Some(SetupAction::Next));

    shell.act(SetupAction::Next);
    let licence = shell.view();
    assert_eq!(licence.page, SetupPage::Licence);
    assert!(!licence.can_next() && licence.can_back());
    shell.act(SetupAction::Toggle(SetupCheck::AcceptLicence));
    assert!(shell.view().licence_accepted && shell.view().can_next());

    let outs = shell.act(SetupAction::Next);
    assert!(matches!(&outs[..], [Out::CheckLater(c)] if *c == candidate(ROOT)));
    let checking = shell.view();
    assert_eq!(checking.page, SetupPage::Folder);
    assert!(!checking.can_next() && !checking.folder_locked);
    assert_eq!(checking.folder_text, ROOT);
}

#[test]
fn a_fresh_install_runs_from_a_ready_folder_to_finish() {
    let shell = &mut shell_on_ready_folder();
    let ready = shell.view();
    assert!(ready.can_next());
    assert_eq!(
        (ready.space_needed, ready.space_free),
        (Some(200), Some(9_000))
    );

    shell.act(SetupAction::Next);
    assert_eq!(shell.view().page, SetupPage::Shortcuts);
    assert_eq!(shell.view().primary, Some(SetupAction::Primary));
    shell.act(SetupAction::Toggle(SetupCheck::Desktop));
    assert!(!shell.view().desktop && shell.view().start_menu);

    let outs = shell.act(SetupAction::Primary);
    let [Out::Send(Job::Install(request))] = &outs[..] else {
        panic!("Install must send one job, got {outs:?}");
    };
    assert_eq!(request.root, PathBuf::from(ROOT));
    assert!(!request.shortcuts.desktop && request.shortcuts.start_menu);
    let installing = shell.view();
    assert_eq!(installing.page, SetupPage::Installing);
    assert!(!installing.can_back() && !installing.can_next() && installing.primary.is_none());
    let progress = Progress::from_elapsed(Duration::from_secs(4), false);
    shell.reply(Reply::Progress(progress));
    assert_eq!(
        shell.view().progress.unwrap().elapsed,
        Duration::from_secs(4)
    );

    shell.reply(exited(SetupExit::Installed(Completion::NavoFinishedUpdate)));
    let finish = shell.view();
    assert_eq!(finish.page, SetupPage::Finish);
    assert_eq!(finish.primary, Some(SetupAction::Primary));
    assert!(finish.notice.is_some() && finish.progress.is_none());
}

#[test]
fn failed_pages_name_the_problem_and_a_partial_root() {
    let mut shell = shell_on_ready_folder();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Primary);
    shell.reply(exited(SetupExit::Failed(Failure::PartialRoot(Some(5)))));
    let failed = shell.view();
    assert_eq!(failed.page, SetupPage::Failed);
    assert_eq!(failed.leftover_root, Some(PathBuf::from(ROOT)));
    assert!(failed.problem.as_deref().unwrap().contains("code 5"));
    assert!(!failed.can_retry && failed.can_back());
    assert_eq!(failed.primary, Some(SetupAction::Close));
    assert!(failed.details.contains("velopack.log"));
    let outs = shell.act(SetupAction::CopyDetails);
    assert!(matches!(&outs[..], [Out::Copy(text)] if text.contains("code 5")));
}

#[test]
fn an_exit_zero_verification_failure_shows_recovery_text_without_a_leftover_alert() {
    let mut shell = shell_on_ready_folder();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Primary);
    shell.reply(exited(SetupExit::Failed(Failure::PartialRoot(Some(0)))));
    let failed = shell.view();
    let problem = failed.problem.as_deref().unwrap_or_default();
    assert!(
        problem.contains("Setup could not finish checking Navo in"),
        "{problem}"
    );
    assert!(
        problem.contains("Remove Navo in Windows Settings → Apps"),
        "{problem}"
    );
    assert!(!problem.contains("Delete this folder"), "{problem}");
    assert_eq!(failed.leftover_root, None);
}

#[test]
fn refusal_pages_come_from_elevation_payload_and_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let page = |host: &mut FakeHost, payload: Option<&Payload>| {
        Shell::new(initial_flow(host, payload, ROOT.into()), facts()).view()
    };

    let mut elevated = FakeHost::new();
    elevated
        .elevated
        .push_back(Err(std::io::Error::other("token")));
    let view = page(&mut elevated, Some(&payload));
    assert_eq!(view.page, SetupPage::Elevated);
    assert_eq!(view.primary, Some(SetupAction::Close));

    assert_eq!(page(&mut FakeHost::new(), None).page, SetupPage::NoPayload);

    let mut broken = FakeHost::new();
    broken
        .discoveries
        .push_back(Err(std::io::Error::other("registry")));
    let view = page(&mut broken, Some(&payload));
    assert_eq!(view.page, SetupPage::Ambiguous);
    assert!(!view.can_back() && view.primary == Some(SetupAction::Close));
}

#[test]
fn an_upgrade_locks_the_folder_and_installs_from_it() {
    let mut shell = upgrade_shell();
    assert_eq!(shell.view().installed_version.as_deref(), Some("0.3.0"));
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Toggle(SetupCheck::AcceptLicence));
    assert!(
        shell.act(SetupAction::Next).is_empty(),
        "an upgrade root is never checked"
    );
    let folder = shell.view();
    assert!(folder.folder_locked && !folder.can_next());
    assert_eq!(folder.folder_text, OTHER);
    assert_eq!(folder.primary, Some(SetupAction::Primary));
    let outs = shell.act(SetupAction::Primary);
    assert!(matches!(&outs[..], [Out::Send(Job::Install(r))] if r.root == Path::new(OTHER)));
}

#[test]
fn a_stale_check_result_is_dropped() {
    let mut shell = shell_on_ready_folder();
    shell.handle(Event::SetFolder(OTHER.into()));
    shell.reply(Reply::Checked {
        candidate: candidate(ROOT),
        result: Ok(()),
        warning: None,
        free: Some(1),
    });
    let view = shell.view();
    assert!(
        !view.can_next(),
        "ROOT's late result must not approve OTHER"
    );
    assert_eq!(view.space_free, None);
    assert_eq!(view.folder_text, OTHER);

    shell.reply(Reply::Checked {
        candidate: candidate(OTHER),
        result: Err(FolderProblem::NotWritable),
        warning: None,
        free: Some(7),
    });
    let view = shell.view();
    assert!(!view.can_next());
    assert_eq!(view.space_free, Some(7));
    assert!(
        view.problem
            .unwrap()
            .starts_with(FolderProblem::NotWritable.message())
    );
}

#[test]
fn an_open_parent_shows_a_warning_with_next_still_on() {
    let mut shell = shell_checking_folder();
    shell.reply(Reply::Checked {
        candidate: candidate(ROOT),
        result: Ok(()),
        warning: Some(FolderWarning::ParentOpen),
        free: Some(9_000),
    });
    let view = shell.view();
    assert_eq!(view.problem, None);
    assert_eq!(
        view.notice.as_deref(),
        Some(FolderWarning::ParentOpen.message())
    );
    assert!(view.can_next());
}

#[test]
fn a_refusal_wins_over_the_parent_warning() {
    let mut shell = shell_checking_folder();
    shell.reply(Reply::Checked {
        candidate: candidate(ROOT),
        result: Err(FolderProblem::NotWritable),
        warning: Some(FolderWarning::ParentOpen),
        free: Some(9_000),
    });
    let view = shell.view();
    assert!(!view.can_next());
    assert_eq!(view.notice, None);
    assert!(
        view.problem
            .unwrap()
            .starts_with(FolderProblem::NotWritable.message())
    );
}

#[test]
fn close_is_ignored_while_installing() {
    let mut shell = shell_on_ready_folder();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Primary);
    assert!(shell.act(SetupAction::Close).is_empty());
    assert!(shell.handle(Event::Esc).is_empty());
    assert_eq!(shell.view().page, SetupPage::Installing);
    assert!(!shell.closing());

    shell.reply(exited(SetupExit::Failed(Failure::Changed)));
    let outs = shell.act(SetupAction::Close);
    assert!(matches!(&outs[..], [Out::Send(Job::Close)]));
    assert!(shell.closing());
    assert!(matches!(&shell.reply(Reply::Closed)[..], [Out::Quit]));
}

#[test]
fn a_lost_worker_fails_the_install_and_close_then_quits() {
    let mut shell = shell_on_ready_folder();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Primary);
    assert!(shell.worker_lost().is_empty());
    assert_eq!(shell.view().page, SetupPage::Failed);
    shell.act(SetupAction::Close);
    assert!(matches!(&shell.worker_lost()[..], [Out::Quit]));
}

/// The test's side of the worker: its inbox, and the reply sender that keeps it "alive".
type FakeWorker = (
    mpsc::Receiver<Job>,
    futures::channel::mpsc::UnboundedSender<Reply>,
);

/// Opens the window on a ready Folder page and returns it with the fake worker's channels.
fn open_on_folder(cx: &mut TestAppContext) -> (Entity<SetupWindow>, FakeWorker) {
    let (jobs, inbox) = mpsc::channel();
    let (outbox, replies) = futures::channel::mpsc::unbounded();
    let shell = shell_on_ready_folder();
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        navo_ui::tokens::apply(navo_ui::tokens::Mode::Light, cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SetupWindow::new(shell, Link { jobs, replies }, window, cx))
        })
        .unwrap()
    });
    (window.1, (inbox, outbox))
}

#[gpui_kit::test]
fn typing_checks_the_folder_once_after_the_pause(cx: &mut TestAppContext) {
    let (window, (inbox, _outbox)) = open_on_folder(cx);
    let type_path = |cx: &mut TestAppContext, path: &str| {
        let event = Event::SetFolder(path.into());
        cx.update(|cx| window.update(cx, |view, cx| view.handle(event, cx)));
    };
    type_path(cx, r"D:\A");
    cx.executor().advance_clock(DEBOUNCE / 2);
    type_path(cx, OTHER);
    cx.executor()
        .advance_clock(DEBOUNCE - Duration::from_millis(1));
    cx.run_until_parked();
    assert!(inbox.try_recv().is_err(), "no check before the pause ends");
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    let checks: Vec<Job> = inbox.try_iter().collect();
    assert!(matches!(&checks[..], [Job::Check(c)] if *c == candidate(OTHER)));
}

#[gpui_kit::test]
fn close_during_install_never_reaches_the_worker(cx: &mut TestAppContext) {
    let (window, (inbox, _outbox)) = open_on_folder(cx);
    cx.update(|cx| {
        window.update(cx, |view, cx| {
            view.act(SetupAction::Next, cx);
            view.act(SetupAction::Primary, cx);
            view.act(SetupAction::Close, cx);
        });
    });
    cx.run_until_parked();
    assert!(matches!(inbox.try_recv(), Ok(Job::Install(_))));
    assert!(
        inbox.try_recv().is_err(),
        "Close must not reach the worker while Installing"
    );
    let page = cx.update(|cx| window.read(cx).shell().page());
    assert_eq!(page, crate::flow::Page::Installing);
}

#[gpui_kit::test]
fn a_worker_that_vanishes_mid_install_shows_failed(cx: &mut TestAppContext) {
    let (window, (inbox, outbox)) = open_on_folder(cx);
    cx.update(|cx| {
        window.update(cx, |view, cx| {
            view.act(SetupAction::Next, cx);
            view.act(SetupAction::Primary, cx);
        });
    });
    drop((inbox, outbox));
    cx.run_until_parked();
    let page = cx.update(|cx| window.read(cx).shell().page());
    assert_eq!(page, crate::flow::Page::Failed);
}
