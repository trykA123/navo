use rusty_pdf::protocol::PdfError;

use super::doc::Wants;
use super::jobs::{self, Done};
use super::{PdfView, Phase, phase_of};
use crate::prelude::*;

const WRONG_PASSWORD: &str = "Wrong password. Check Caps Lock and try again.";

impl PdfView {
    fn wants(&self, cx: &App) -> Wants {
        Wants {
            index: !self.query.is_empty(),
            thumbs: self.left.read(cx).is_open(),
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
        let (read, dropped) = doc.apply(done);
        for image in dropped {
            cx.drop_image(image, None);
        }
        if let Some(page) = read {
            self.search_page(page);
        }
        cx.notify();
        self.pump(cx);
    }

    pub(super) fn unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.phase, Phase::Locked { .. }) {
            return;
        }
        let password = self.inputs.password.read(cx).value().to_string();
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
            opened => phase_of(opened),
        };
        if matches!(self.phase, Phase::Ready(_)) {
            self.focus_pages(window, cx);
        }
        cx.notify();
    }
}
