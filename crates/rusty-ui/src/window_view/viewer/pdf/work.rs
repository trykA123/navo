use rusty_pdf::protocol::{MAX_PASSWORD_BYTES, PdfError};

use super::doc::Applied;
use super::doc::Wants;
use super::jobs::{self, Done};
use super::trouble::Verdict;
use super::{LeftTab, PdfView, Phase, phase_of};
use crate::prelude::*;

const WRONG_PASSWORD: &str = "Wrong password. Check Caps Lock and try again.";
const ASK_AGAIN: &str = "The PDF helper restarted. Enter the password again.";

impl PdfView {
    fn wants(&self, cx: &App) -> Wants {
        let first_only = self.is_preview && !self.is_active;
        Wants {
            index: !self.query.is_empty() && !first_only,
            thumbs: self.left.read(cx).is_open() && self.left_tab == LeftTab::Pages,
            chapters: self.left.read(cx).is_open() && self.left_tab == LeftTab::Outline,
            first_only,
        }
    }

    pub(super) fn pump(&mut self, cx: &mut Context<Self>) {
        let wants = self.wants(cx);
        let Some(doc) = self.doc_mut() else {
            return;
        };
        if doc.busy.is_some() {
            return;
        }
        let Some(job) = doc.next_job(&wants) else {
            return;
        };
        let shared = doc.shared();
        doc.busy = Some(cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move { jobs::run(&shared, job) })
                .await;
            let _ = this.update(cx, |view, cx| view.finish(done, cx));
        }));
    }

    fn finish(&mut self, done: Done, cx: &mut Context<Self>) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        doc.busy = None;
        let Applied {
            read,
            dropped,
            verdict,
        } = doc.apply(done);
        for image in dropped {
            cx.drop_image(image, None);
        }
        if let Some(page) = read {
            self.search_page(page);
        }
        match verdict {
            Some(Verdict::Relock) => self.relock(ASK_AGAIN, cx),
            Some(Verdict::Refuse(error)) => self.refuse(error, cx),
            None => self.pump(cx),
        }
        cx.notify();
    }

    fn drop_doc(&mut self, cx: &mut Context<Self>) {
        let images = self
            .doc_mut()
            .map(super::doc::Doc::drain_images)
            .unwrap_or_default();
        for image in images {
            cx.drop_image(image, None);
        }
        self.drawers.clear();
        self.searching = None;
    }

    fn relock(&mut self, error: &'static str, cx: &mut Context<Self>) {
        self.drop_doc(cx);
        self.phase = Phase::Locked {
            error: Some(error.into()),
        };
    }

    fn refuse(&mut self, error: PdfError, cx: &mut Context<Self>) {
        self.drop_doc(cx);
        self.phase = Phase::Refused(error);
    }

    pub(super) fn unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.phase, Phase::Locked { .. }) {
            return;
        }
        let password = self.inputs.password.read(cx).value().to_string();
        if password.len() > MAX_PASSWORD_BYTES {
            return self.unlocked(Err(PdfError::WrongPassword), window, cx);
        }
        let (open, location) = (self.open.clone(), self.location.clone());
        let task = cx.spawn_in(window, async move |this, cx| {
            let opened = cx
                .background_spawn(async move { open(&location, Some(password.as_str())) })
                .await;
            let _ = this.update_in(cx, |view, window, cx| view.unlocked(opened, window, cx));
        });
        self.phase = Phase::Unlocking { _task: task };
        cx.notify();
    }

    fn unlocked(
        &mut self,
        opened: Result<Box<dyn super::PdfPages>, PdfError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.phase = match opened {
            Err(PdfError::WrongPassword) => Phase::Locked {
                error: Some(WRONG_PASSWORD.into()),
            },
            opened => phase_of(opened, self.is_preview),
        };
        let is_ready = matches!(self.phase, Phase::Ready(_));
        self.inputs.password.update(cx, |input, cx| {
            if is_ready {
                input.set_value("", window, cx);
            } else {
                input.select_all(window, cx);
            }
        });
        if is_ready {
            self.focus_pages(window, cx);
        }
        cx.notify();
    }
}
