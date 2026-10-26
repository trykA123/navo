use rusty_ui::crash_notice::RecoveryContent;
use rusty_ui::fkey_bar::KeyLayer;
use rusty_ui::prelude::*;

use super::{HintsShown, JobsLook, JobsShown, WindowPage};

impl WindowPage {
    pub fn with_recovery(cx: &mut Context<Self>) -> Self {
        let mut page = Self::with_crash(cx);
        page.recovery = Some(RecoveryContent {
            title: "Navo recovered after a database problem.".into(),
            details: vec![
                "The damaged database was preserved as navo.db.corrupt-1.".into(),
                "Readable saved data was kept.".into(),
                "Reset: folder history, search history.".into(),
                "Recovery completed for 2 operations. Operations needing manual attention: 1."
                    .into(),
                "D:/Projects/module/config.xml.navo-held".into(),
                "Full recovery details: navo-recovery-1-1.jsonl.".into(),
            ],
        });
        page
    }

    pub fn with_jobs(shown: JobsShown, cx: &mut Context<Self>) -> Self {
        let mut page = Self::new(None, cx);
        page.parts.fkey_bar = shown.look == JobsLook::Card;
        Self {
            jobs: Some(shown),
            ..page
        }
    }

    pub fn with_hints(shown: HintsShown, held: KeyLayer, cx: &mut Context<Self>) -> Self {
        Self {
            hints: Some(shown),
            ..Self::new(Some(held), cx)
        }
    }

    pub fn with_crash(cx: &mut Context<Self>) -> Self {
        Self {
            has_crash: true,
            ..Self::new(None, cx)
        }
    }

    pub fn with_menu(cx: &mut Context<Self>) -> Self {
        Self {
            is_menu_open: true,
            ..Self::new(None, cx)
        }
    }
}
