//! Gallery entries for the installer wizard: each page and state, built from fixed `SetupView`s.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use navo_ui::prelude::*;
use navo_ui::setup::{
    SetupAction, SetupPage, SetupProgress, SetupStage, SetupView, Strings, render_page,
};
use navo_ui::tokens;

use crate::pages::Page;

const FOLDER: &str = r"C:\Users\admin\AppData\Local\Navo";
/// A longer Finish folder, so the gallery shows the one-line path cut with an ellipsis.
const FINISH_FOLDER: &str = r"C:\Users\admin\AppData\Local\Programs\Navo";
const WIDTH: f32 = 640.;
const HEIGHT: f32 = 480.;

/// The shared start of every view: a page, the version and the install folder.
fn base(page: SetupPage) -> SetupView {
    SetupView {
        page,
        can_back: true,
        version: "0.4.0".into(),
        folder_text: FOLDER.into(),
        ..SetupView::default()
    }
}

/// A view that can go on, with Next as its primary button.
fn next(page: SetupPage) -> SetupView {
    SetupView {
        can_next: true,
        primary: Some(SetupAction::Next),
        ..base(page)
    }
}

/// A view whose primary button is `action` (Install, Finish, Open Navo or a refusal's Close).
fn primary(page: SetupPage, action: SetupAction) -> SetupView {
    SetupView {
        primary: Some(action),
        can_back: false,
        ..base(page)
    }
}

/// The Installing page after `seconds`, with the runner's 180-second notice flag.
fn installing(seconds: u64) -> SetupView {
    SetupView {
        can_back: false,
        progress: Some(SetupProgress::from_elapsed(
            Duration::from_secs(seconds),
            seconds >= 180,
        )),
        log_paths: vec![PathBuf::from(format!(r"{FOLDER}\setup.log"))],
        ..base(SetupPage::Installing)
    }
}

/// The Installing page with a verified share and phase, for the bar's determinate states.
fn installing_with(seconds: u64, percent: Option<u8>, stage: SetupStage) -> SetupView {
    SetupView {
        progress: Some(
            SetupProgress::from_elapsed(Duration::from_secs(seconds), false)
                .with_estimate(percent, Some(stage)),
        ),
        ..installing(seconds)
    }
}

/// The Failed page; `leftover` names a fresh install's root that still holds files.
fn failed(leftover: bool) -> SetupView {
    SetupView {
        can_back: false,
        can_retry: !leftover,
        leftover_root: leftover.then(|| PathBuf::from(FOLDER)),
        log_paths: vec![PathBuf::from(format!(r"{FOLDER}\setup.log"))],
        details: "step: copy files\nerror: access denied (os error 5)".into(),
        primary: Some(SetupAction::Close),
        ..base(SetupPage::Failed)
    }
}

/// A refusal page that offers only Close, with the installed version set.
fn refusal(page: SetupPage) -> SetupView {
    SetupView {
        installed_version: Some("0.5.0".into()),
        ..primary(page, SetupAction::Close)
    }
}

/// Returns the fixed view for a setup gallery page, or `None` for any other page.
fn view(page: Page) -> Option<SetupView> {
    Some(match page {
        Page::SetupWelcome => next(SetupPage::Welcome),
        Page::SetupWelcomeUpdate => SetupView {
            installed_version: Some("0.3.0".into()),
            ..next(SetupPage::Welcome)
        },
        Page::SetupLicence => SetupView {
            can_next: false,
            ..next(SetupPage::Licence)
        },
        Page::SetupLicenceAccepted => SetupView {
            licence_accepted: true,
            ..next(SetupPage::Licence)
        },
        Page::SetupFolder => SetupView {
            space_needed: Some(180 << 20),
            space_free: Some(96 << 30),
            ..next(SetupPage::Folder)
        },
        Page::SetupFolderLocked => SetupView {
            folder_locked: true,
            installed_version: Some("0.3.0".into()),
            ..next(SetupPage::Folder)
        },
        Page::SetupFolderProblem => SetupView {
            problem: Some("This folder is not empty and does not hold Navo.".into()),
            ..next(SetupPage::Folder)
        },
        Page::SetupShortcuts => SetupView {
            start_menu: true,
            desktop: true,
            primary: Some(SetupAction::Primary),
            ..next(SetupPage::Shortcuts)
        },
        _ => return view_after_shortcuts(page),
    })
}

/// Returns the fixed view for an Installing, Finish, Failed or refusal gallery page.
fn view_after_shortcuts(page: Page) -> Option<SetupView> {
    Some(match page {
        Page::SetupInstalling => installing(12),
        Page::SetupInstallingPercent => installing_with(12, Some(35), SetupStage::Extracting),
        Page::SetupInstallingFinalizing => installing_with(26, None, SetupStage::Finalizing),
        Page::SetupInstallingSlow => installing(185),
        Page::SetupFinish => SetupView {
            start_navo: true,
            folder_text: FINISH_FOLDER.into(),
            ..primary(SetupPage::Finish, SetupAction::Primary)
        },
        Page::SetupFailedRetry => failed(false),
        Page::SetupFailedLeftover => failed(true),
        Page::SetupBlocked => primary(SetupPage::Blocked, SetupAction::Close),
        Page::SetupBlockedRetry => SetupView {
            can_retry: true,
            ..primary(SetupPage::Blocked, SetupAction::Close)
        },
        Page::SetupSame => refusal(SetupPage::Same),
        Page::SetupNewer => refusal(SetupPage::Newer),
        Page::SetupAmbiguous => refusal(SetupPage::Ambiguous),
        Page::SetupAmbiguousNotice => SetupView {
            notice: Some("The folder name has a character Setup cannot repair.".into()),
            ..refusal(SetupPage::Ambiguous)
        },
        Page::SetupElevated => primary(SetupPage::Elevated, SetupAction::Close),
        Page::SetupNoPayload => primary(SetupPage::NoPayload, SetupAction::Close),
        _ => return None,
    })
}

/// Returns the page for a setup gallery entry: the wizard body in a 640 x 480 frame.
pub fn element(page: Page, cx: &App) -> Option<AnyElement> {
    let view = view(page)?;
    let t = tokens::current(cx);
    let body = render_page(&view, &Strings::english(), Rc::new(|_, _, _| {}));
    let frame = div()
        .flex_none()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .overflow_hidden()
        .p(t.spacing.xl)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.colors.surface)
        .child(body);
    Some(
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .bg(t.colors.background)
            .child(frame)
            .into_any_element(),
    )
}
