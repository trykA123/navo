//! Recovery tests for the window core: Back from Failed, a lost worker, invalid edits and unsafe-name text.

use navo_ui::setup::{SetupAction, SetupPage};

use crate::flow::{Event, Failure, InstallAction, SetupExit};
use crate::mode::{Mode, Why};
use crate::window_core::{Out, Shell};
use crate::window_tests::{
    ROOT, candidate, exited, fresh_shell, shell_on_ready_folder, upgrade_shell,
};
use crate::window_worker::{Job, Reply};

/// Walks a ready fresh shell through Install to a failed run.
fn failed_fresh(failure: Failure) -> Shell {
    let mut shell = shell_on_ready_folder();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Primary);
    shell.reply(exited(SetupExit::Failed(failure)));
    assert_eq!(shell.view().page, SetupPage::Failed);
    shell
}

/// Whether `outs` is exactly one Install job with `action`.
fn installs(outs: &[Out], action: InstallAction) -> bool {
    matches!(outs, [Out::Send(Job::Install(r))] if r.action == action)
}

#[test]
fn back_from_an_empty_root_releases_it_and_checks_the_folder_again() {
    for failure in [Failure::EmptyRoot(Some(1)), Failure::Extract] {
        let mut shell = failed_fresh(failure);
        let outs = shell.act(SetupAction::Back);
        assert!(
            matches!(&outs[..], [Out::Send(Job::Release), Out::CheckLater(c)] if *c == candidate(ROOT)),
            "Back must release the held root before the re-check, got {outs:?}"
        );
        shell.reply(Reply::Checked {
            candidate: candidate(ROOT),
            result: Ok(()),
            free: Some(9_000),
        });
        shell.act(SetupAction::Next);
        let outs = shell.act(SetupAction::Primary);
        assert!(installs(&outs, InstallAction::Run), "got {outs:?}");
    }
}

#[test]
fn back_from_a_failed_upgrade_can_install_again() {
    let mut shell = upgrade_shell();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Toggle(
        navo_ui::setup::SetupCheck::AcceptLicence,
    ));
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Primary);
    shell.reply(exited(SetupExit::Failed(Failure::Upgrade(Some(2)))));
    let outs = shell.act(SetupAction::Back);
    assert!(
        matches!(&outs[..], [Out::Send(Job::Release)]),
        "got {outs:?}"
    );
    let outs = shell.act(SetupAction::Primary);
    assert!(installs(&outs, InstallAction::Run), "got {outs:?}");
}

#[test]
fn a_lost_worker_fails_every_waiting_page() {
    let mut checking = fresh_shell();
    checking.act(SetupAction::Next);
    checking.act(SetupAction::Toggle(
        navo_ui::setup::SetupCheck::AcceptLicence,
    ));
    checking.act(SetupAction::Next);
    assert!(checking.worker_lost().is_empty());
    assert_eq!(
        checking.view().page,
        SetupPage::Failed,
        "Checking must not hang"
    );

    let mut blocked = shell_on_ready_folder();
    blocked.act(SetupAction::Next);
    blocked.act(SetupAction::Primary);
    blocked.reply(exited(SetupExit::Blocked));
    blocked.act(SetupAction::TryAgain);
    blocked.worker_lost();
    assert_eq!(
        blocked.view().page,
        SetupPage::Failed,
        "a pending rescan must not hang"
    );
}

#[test]
fn an_invalid_edit_cancels_the_pending_check_and_clears_free_space() {
    let mut shell = shell_on_ready_folder();
    assert_eq!(shell.view().space_free, Some(9_000));
    let outs = shell.handle(Event::SetFolder(r"C:\a%b\Navo".into()));
    assert!(matches!(&outs[..], [Out::CancelCheck]), "got {outs:?}");
    assert_eq!(shell.view().space_free, None);
}

#[test]
fn the_unsafe_name_notice_never_mentions_uninstalling() {
    let mode = Mode::Ambiguous(Why::UnsafeName {
        root: r"C:\R&D\Navo".into(),
        character: '&',
    });
    let shell = Shell::new(crate::flow::Flow::new(mode, ROOT.into()), facts());
    let notice = shell.view().notice.expect("UnsafeName has its own text");
    assert!(!notice.to_lowercase().contains("uninstall"), "{notice}");
    assert_eq!(
        notice,
        "Navo is installed in `C:\\R&D\\Navo`. Setup cannot update a folder whose name contains `&`. Keep using Navo there; it updates itself."
    );
}

/// The window facts the tests share.
fn facts() -> crate::window_view::Facts {
    crate::window_view::Facts {
        version: "0.4.0".into(),
        space_needed: Some(200),
        log_paths: Vec::new(),
    }
}
