//! Behaviour tests for the pure wizard flow; no runner, UI or filesystem is used.

use super::*;
use crate::mode::{Install, Why, parse_sq_version};
use navo_core::{FileIdentity, FileIdentityKind};

/// Builds a syntax-checked Windows candidate without accessing the filesystem.
fn candidate(path: &str) -> Candidate {
    crate::syntax::parse_candidate(path.as_ref()).unwrap()
}

/// Supplies the checked installation facts that discovery passes to the flow.
fn installed() -> Install {
    Install {
        root: r"C:\Apps\Navo".into(),
        identity: FileIdentity {
            volume: 1,
            file: [2; 16],
            kind: FileIdentityKind::Native,
        },
        safe_chain: true,
        update_exe: true,
        sq_version: parse_sq_version(
            "<package><metadata><id>NavoApp</id><version>0.3.0</version></metadata></package>",
        ),
        marker: None,
    }
}

/// Advances through Welcome and the accepted Licence to Folder.
fn folder(mode: Mode) -> Flow {
    let flow = Flow::new(mode, r"C:\Programs\Navo".into());
    let (flow, _) = step(flow, Event::Next);
    let (flow, _) = step(flow, Event::AcceptLicence(true));
    step(flow, Event::Next).0
}

/// Checks the fresh folder and advances to Shortcuts.
fn shortcuts() -> Flow {
    let flow = folder(Mode::Fresh);
    let (flow, _) = step(
        flow,
        Event::FolderChecked {
            candidate: candidate(r"C:\Programs\Navo"),
            result: Ok(()),
        },
    );
    step(flow, Event::Next).0
}

/// Starts a fresh request and returns its in-flight flow.
fn installing() -> Flow {
    let (flow, effects) = step(shortcuts(), Event::Install);
    assert!(matches!(effects.as_slice(), [Effect::Install(_)]));
    flow
}

/// Licence consent and a successful folder check are both required for Next.
#[test]
fn next_requires_licence_and_a_checked_folder_without_problems() {
    let flow = Flow::new(Mode::Fresh, r"C:\Programs\Navo".into());
    let (flow, _) = step(flow, Event::Next);
    assert_eq!(flow.page(), Page::Licence);
    let (flow, effects) = step(flow, Event::Next);
    assert_eq!(flow.page(), Page::Licence);
    assert!(effects.is_empty());
    let (flow, _) = step(flow, Event::AcceptLicence(true));
    let (flow, effects) = step(flow, Event::Next);
    assert_eq!(
        effects,
        vec![Effect::CheckFolder(candidate(r"C:\Programs\Navo"))]
    );
    let (flow, effects) = step(flow, Event::Next);
    assert_eq!(flow.page(), Page::Folder);
    assert!(effects.is_empty());
    let (flow, _) = step(
        flow,
        Event::FolderChecked {
            candidate: candidate(r"C:\Programs\Navo"),
            result: Err(FolderProblem::NotWritable),
        },
    );
    let (flow, effects) = step(flow, Event::Next);
    assert_eq!(flow.page(), Page::Folder);
    assert!(effects.is_empty());
    assert!(matches!(
        flow.folder(),
        Folder::Refused {
            problem: FolderProblem::NotWritable,
            ..
        }
    ));
}

/// No user action or check result can start another child or leave Installing.
#[test]
fn installing_refuses_close_back_and_a_second_install_until_exit() {
    let mut flow = installing();
    for event in [
        Event::Close,
        Event::Back,
        Event::Install,
        Event::Retry,
        Event::Esc,
        Event::Next,
    ] {
        let (next, effects) = step(flow, event);
        assert_eq!(next.page(), Page::Installing);
        assert!(effects.is_empty());
        flow = next;
    }
    let (flow, _) = step(
        flow,
        Event::SetupExited(SetupExit::Failed(Failure::EmptyRoot(None))),
    );
    let (flow, effects) = step(flow, Event::Retry);
    assert_eq!(flow.page(), Page::Installing);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Install(Request {
            action: InstallAction::RetryEmpty,
            ..
        })]
    ));
}

/// Escape closes each pre-install page without another effect.
#[test]
fn escape_before_install_emits_only_close() {
    let welcome = Flow::new(Mode::Fresh, r"C:\Programs\Navo".into());
    let licence = step(welcome.clone(), Event::Next).0;
    for flow in [welcome, licence, folder(Mode::Fresh), shortcuts()] {
        let (flow, effects) = step(flow, Event::Esc);
        assert_eq!(effects, vec![Effect::Close]);
        assert!(step(flow, Event::Install).1.is_empty());
    }
}

/// Elevation and payload refusals cannot advance, retry, browse or start Navo.
#[test]
fn elevated_and_no_payload_are_terminal() {
    for exit in [SetupExit::Elevated, SetupExit::NoPayload] {
        let (mut flow, _) = step(installing(), Event::SetupExited(exit));
        let page = flow.page();
        for event in [
            Event::Next,
            Event::Back,
            Event::Install,
            Event::Retry,
            Event::OpenNavo,
        ] {
            let (next, effects) = step(flow, event);
            assert_eq!(next.page(), page);
            assert!(effects.is_empty());
            flow = next;
        }
        assert_eq!(step(flow, Event::Close).1, vec![Effect::Close]);
    }
}

/// Upgrade keeps the discovered root, skips Shortcuts and preserves shortcut choices.
#[test]
fn upgrade_locks_the_folder_and_skips_shortcuts() {
    let flow = folder(Mode::Upgrade(installed()));
    assert_eq!(flow.folder(), &Folder::Locked(installed().root));
    for event in [
        Event::SetFolder(r"C:\Other".into()),
        Event::BrowsePicked(r"D:\Apps".into()),
    ] {
        let (next, effects) = step(flow.clone(), event);
        assert_eq!(next.folder(), flow.folder());
        assert!(effects.is_empty());
    }
    assert_eq!(step(flow.clone(), Event::Next).0.page(), Page::Folder);
    let (flow, effects) = step(flow, Event::Install);
    assert_eq!(flow.page(), Page::Installing);
    assert!(
        matches!(&effects[..], [Effect::Install(request)] if matches!(*request.mode, Mode::Upgrade(_)) && request.root == installed().root)
    );
}

/// Fresh edits invalidate earlier checks; Browse adds the Navo child folder.
#[test]
fn fresh_edits_drop_old_candidate_results_and_validate_syntax() {
    let flow = folder(Mode::Fresh);
    let (flow, effects) = step(flow, Event::BrowsePicked(r"D:\Apps".into()));
    let selected = candidate(r"D:\Apps\Navo");
    assert_eq!(effects, vec![Effect::CheckFolder(selected.clone())]);
    let (flow, effects) = step(
        flow,
        Event::FolderChecked {
            candidate: candidate(r"C:\Programs\Navo"),
            result: Ok(()),
        },
    );
    assert_eq!(flow.folder(), &Folder::Checking(selected.clone()));
    assert!(effects.is_empty());
    let (flow, _) = step(
        flow,
        Event::FolderChecked {
            candidate: selected.clone(),
            result: Ok(()),
        },
    );
    let (flow, _) = step(
        flow,
        Event::FolderChecked {
            candidate: candidate(r"C:\Programs\Navo"),
            result: Err(FolderProblem::ProbeFailed),
        },
    );
    assert_eq!(flow.folder(), &Folder::Ready(selected));
    let (flow, effects) = step(flow, Event::SetFolder("relative".into()));
    assert!(effects.is_empty());
    assert!(matches!(
        flow.folder(),
        Folder::Refused {
            problem: FolderProblem::NotAbsolute,
            ..
        }
    ));
    assert_eq!(step(flow, Event::Next).0.page(), Page::Folder);
}

/// Partial, changed or installed recovery roots never start another full install.
#[test]
fn failed_nonempty_roots_disable_retry_even_after_back() {
    for failure in [
        Failure::PartialRoot(None),
        Failure::Changed,
        Failure::AfterExit(None),
        Failure::Remarked,
        Failure::Unlisted,
    ] {
        let (flow, _) = step(installing(), Event::SetupExited(SetupExit::Failed(failure)));
        assert_eq!(flow.page(), Page::Failed);
        assert!(!flow.can_retry());
        assert!(step(flow.clone(), Event::Retry).1.is_empty());
        let (flow, _) = step(flow, Event::Back);
        let (flow, _) = step(flow, Event::Next);
        assert!(step(flow, Event::Install).1.is_empty());
    }
}

/// Marker recovery requests only mark_again through the runner's held session.
#[test]
fn unmarked_retry_only_requests_marker_recovery() {
    let (flow, _) = step(
        installing(),
        Event::SetupExited(SetupExit::Failed(Failure::Unmarked)),
    );
    assert!(flow.can_retry());
    let (_, effects) = step(flow, Event::Retry);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Install(Request {
            action: InstallAction::MarkAgain,
            ..
        })]
    ));
}

/// Same allows Open Navo and Close; navigation never launches a Velopack request.
#[test]
fn same_allows_open_navo_without_installing() {
    let flow = Flow::new(Mode::Same(installed()), r"D:\Ignored".into());
    assert_eq!(flow.page(), Page::Same);
    for event in [Event::Next, Event::Back, Event::Install, Event::Retry] {
        assert!(step(flow.clone(), event).1.is_empty());
    }
    assert_eq!(
        step(flow.clone(), Event::OpenNavo).1,
        vec![Effect::StartNavo(installed().root), Effect::Close]
    );
    assert_eq!(step(flow, Event::Close).1, vec![Effect::Close]);
}

/// Newer and every ambiguous reason refuse all installation and launch actions.
#[test]
fn newer_and_ambiguous_offer_only_close() {
    let reasons = [
        Why::InvalidFolder,
        Why::MultipleInstalls,
        Why::DamagedVersion,
        Why::ForeignVersion,
        Why::MissingMarker,
        Why::InvalidMarker,
        Why::UnsafeName {
            root: r"C:\Bad%\Navo".into(),
            character: '%',
        },
    ];
    let modes =
        std::iter::once(Mode::Newer(installed())).chain(reasons.into_iter().map(Mode::Ambiguous));
    for mode in modes {
        let flow = Flow::new(mode.clone(), r"D:\Ignored".into());
        assert_eq!(flow.mode(), &mode);
        assert!(matches!(flow.page(), Page::Newer | Page::Ambiguous));
        for event in [
            Event::Next,
            Event::Back,
            Event::Install,
            Event::Retry,
            Event::OpenNavo,
        ] {
            assert!(step(flow.clone(), event).1.is_empty());
        }
        assert_eq!(step(flow, Event::Close).1, vec![Effect::Close]);
    }
}

/// Blocked retries scan first; both running processes and scan errors fail closed.
#[test]
fn blocked_rescans_and_installs_only_after_a_clear_scan() {
    let (flow, _) = step(folder(Mode::Upgrade(installed())), Event::Install);
    let (mut flow, _) = step(flow, Event::SetupExited(SetupExit::Blocked));
    assert_eq!(flow.page(), Page::Blocked);
    for result in [RunningScan::Running, RunningScan::Failed] {
        let (next, effects) = step(flow, Event::Retry);
        assert_eq!(effects, vec![Effect::RescanRunning]);
        assert!(step(next.clone(), Event::Retry).1.is_empty());
        let (next, effects) = step(next, Event::RunningScanned(result));
        assert_eq!(next.page(), Page::Blocked);
        assert!(effects.is_empty());
        flow = next;
    }
    let (flow, _) = step(flow, Event::Retry);
    let (flow, effects) = step(flow, Event::RunningScanned(RunningScan::Clear));
    assert_eq!(flow.page(), Page::Installing);
    assert!(matches!(effects.as_slice(), [Effect::Install(_)]));
}

/// A verified self-update reaches Finish with the W1 success notice.
#[test]
fn mid_upgrade_success_adds_the_navo_finished_notice() {
    let (flow, _) = step(folder(Mode::Upgrade(installed())), Event::Install);
    let (flow, effects) = step(
        flow,
        Event::SetupExited(SetupExit::Installed(Completion::NavoFinishedUpdate)),
    );
    assert_eq!(flow.page(), Page::Finish);
    assert_eq!(flow.completion(), Some(Completion::NavoFinishedUpdate));
    assert_eq!(
        Completion::NavoFinishedUpdate.notice(),
        Some("Navo finished the update itself.")
    );
    assert!(effects.is_empty());
}

/// An unverified self-update stays failed until a retry scan confirms Navo closed.
#[test]
fn mid_upgrade_failure_requires_a_clear_scan_before_retry() {
    let (flow, _) = step(folder(Mode::Upgrade(installed())), Event::Install);
    let (flow, effects) = step(
        flow,
        Event::SetupExited(SetupExit::Failed(Failure::NavoStarted)),
    );
    assert_eq!(flow.page(), Page::Failed);
    assert_eq!(flow.failure(), Some(&Failure::NavoStarted));
    assert_eq!(
        Failure::NavoStarted.notice(),
        Some("Navo was started during Setup. Close Navo and run Setup again.")
    );
    assert_eq!(effects, vec![Effect::RescanRunning]);
    assert!(!flow.can_retry());
    assert!(step(flow.clone(), Event::Retry).1.is_empty());
    let (back, _) = step(flow.clone(), Event::Back);
    assert!(
        step(back, Event::RunningScanned(RunningScan::Clear))
            .1
            .is_empty()
    );
    let (flow, effects) = step(flow, Event::RunningScanned(RunningScan::Running));
    assert_eq!(flow.page(), Page::Failed);
    assert!(effects.is_empty());
    assert!(!flow.can_retry());
    let (flow, _) = step(flow, Event::Rescan);
    let (flow, effects) = step(flow, Event::RunningScanned(RunningScan::Clear));
    assert!(effects.is_empty());
    assert!(flow.can_retry());
    assert!(matches!(
        step(flow, Event::Retry).1.as_slice(),
        [Effect::Install(_)]
    ));
}

/// Shortcut selections reach the request; Finish starts Navo only when selected.
#[test]
fn shortcut_choices_and_start_navo_choices_reach_effects() {
    let choices = Shortcuts {
        desktop: false,
        start_menu: true,
    };
    let (flow, _) = step(shortcuts(), Event::SetShortcuts(choices));
    let (flow, effects) = step(flow, Event::Install);
    assert!(
        matches!(effects.as_slice(), [Effect::Install(Request { shortcuts, .. })] if *shortcuts == choices)
    );
    let (flow, _) = step(
        flow,
        Event::SetupExited(SetupExit::Installed(Completion::Installed)),
    );
    assert_eq!(
        step(flow.clone(), Event::Finish).1,
        vec![Effect::StartNavo(r"C:\Programs\Navo".into()), Effect::Close]
    );
    let (flow, _) = step(flow, Event::SetStartNavo(false));
    assert_eq!(step(flow, Event::Finish).1, vec![Effect::Close]);
}
