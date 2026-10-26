use rusty_l10n::{t, tr};

use super::Failure;
use super::start::{Ask, Choice, Prompt};
use crate::splash::Splash;

pub(crate) struct Dialogs<'a> {
    pub(crate) splash: &'a mut Splash,
}

impl Prompt for Dialogs<'_> {
    fn ask(&mut self, ask: Ask, failure: Option<&Failure>) -> Choice {
        self.splash.close();
        show(&Text::new(ask, failure))
    }
}

pub(super) struct Text {
    pub(super) heading: String,
    pub(super) detail: String,
    #[cfg_attr(
        not(any(windows, test)),
        expect(dead_code, reason = "only the Windows dialog shows buttons")
    )]
    pub(super) choices: Vec<(Choice, String)>,
}

impl Text {
    pub(super) fn new(ask: Ask, failure: Option<&Failure>) -> Self {
        let try_again = (Choice::TryAgain, tr("migration-try-again"));
        let quit = (Choice::Quit, tr("migration-quit"));
        match ask {
            Ask::Blocked => Self {
                heading: tr("migration-blocked"),
                detail: tr("migration-blocked-detail"),
                choices: vec![try_again, quit],
            },
            Ask::InUse => Self {
                heading: tr("migration-in-use"),
                detail: tr("migration-in-use-detail"),
                choices: vec![try_again, quit],
            },
            Ask::Failed { can_start_fresh } => {
                let error = failure.map_or_else(String::new, |failure| failure.error.to_string());
                let mut choices = vec![try_again];
                let detail = if can_start_fresh {
                    choices.push((Choice::StartFresh, tr("migration-start-fresh")));
                    t!("migration-failed-detail-fresh", error = error)
                } else {
                    t!("migration-failed-detail", error = error)
                };
                choices.push(quit);
                Self {
                    heading: tr("migration-failed"),
                    detail,
                    choices,
                }
            }
        }
    }
}

#[cfg(not(windows))]
fn show(text: &Text) -> Choice {
    eprintln!("navo: {}\n{}", text.heading, text.detail);
    Choice::Quit
}

#[cfg(windows)]
fn show(text: &Text) -> Choice {
    use windows_sys::Win32::UI::Controls::{
        TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TD_WARNING_ICON,
        TDF_ALLOW_DIALOG_CANCELLATION, TaskDialogIndirect,
    };

    const FIRST_ID: i32 = 100;
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let title = wide(rusty_core::brand::NAME);
    let heading = wide(&text.heading);
    let detail = wide(&text.detail);
    let labels: Vec<Vec<u16>> = text.choices.iter().map(|(_, label)| wide(label)).collect();
    let buttons: Vec<TASKDIALOG_BUTTON> = labels
        .iter()
        .zip(FIRST_ID..)
        .map(|(label, id)| TASKDIALOG_BUTTON {
            nButtonID: id,
            pszButtonText: label.as_ptr(),
        })
        .collect();
    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION,
        pszWindowTitle: title.as_ptr(),
        Anonymous1: TASKDIALOGCONFIG_0 {
            pszMainIcon: TD_WARNING_ICON,
        },
        pszMainInstruction: heading.as_ptr(),
        pszContent: detail.as_ptr(),
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        nDefaultButton: FIRST_ID,
        ..TASKDIALOGCONFIG::default()
    };
    let mut pressed = 0;
    // SAFETY: every pointer in the config points into a local buffer that outlives the modal call.
    let result = unsafe {
        TaskDialogIndirect(
            &config,
            &mut pressed,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result < 0 {
        tracing::error!(result, "cannot show the profile move dialog");
        return Choice::Quit;
    }
    pressed
        .checked_sub(FIRST_ID)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| text.choices.get(index))
        .map_or(Choice::Quit, |(choice, _)| *choice)
}
