use rusty_ui::fkey_bar::KeyLayer;
use rusty_ui::prelude::*;

use super::{HintsShown, JobsLook, JobsShown, WindowPage};

impl WindowPage {
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
