use rusty_ui::confirm_dialog::{ConfirmDialog, ConfirmProps};
use rusty_ui::job_row::{JobRow, JobRowProps, JobRowState};
use rusty_ui::prelude::*;
use rusty_ui::tokens::{self, Theme};

use crate::sections::section;

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

pub struct OperatePage {
    last: SharedString,
}

impl OperatePage {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            last: "No answer yet".into(),
        }
    }

    fn answer(
        &self,
        id: &'static str,
        props: ConfirmProps,
        cx: &mut Context<Self>,
    ) -> ConfirmDialog {
        let confirm = cx.entity();
        let cancel = cx.entity();
        ConfirmDialog::new(id, props)
            .on_confirm(move |_, cx| {
                confirm.update(cx, |this, cx| this.set_last("Confirmed", cx));
            })
            .on_cancel(move |_, cx| {
                cancel.update(cx, |this, cx| this.set_last("Cancelled", cx));
            })
    }

    fn set_last(&mut self, text: &'static str, cx: &mut Context<Self>) {
        self.last = text.into();
        cx.notify();
    }
}

fn permanent() -> ConfirmProps {
    ConfirmProps {
        title: "Delete 8 items permanently?".into(),
        lines: [
            r"D:\work\commander\target\debug\ferrite.exe",
            r"D:\work\commander\target\debug\ferrite.pdb",
            r"D:\work\commander\notes\2026-10-14 operate.md",
            r"D:\work\commander\design\screens\b4-1440.png",
            r"D:\work\commander\old",
            r"D:\work\commander\scratch.txt",
        ]
        .map(SharedString::from)
        .to_vec(),
        more: 2,
        summary: "7 files, 1 folder · 412,880,113 B".into(),
        confirm_label: "Delete permanently".into(),
        danger: true,
        typed: None,
    }
}

fn protected() -> ConfirmProps {
    ConfirmProps {
        title: r"Delete C:\Program Files\Ferrite permanently?".into(),
        lines: vec![r"C:\Program Files\Ferrite".into()],
        more: 0,
        summary: "This is a protected location · 1 folder · 38,112,004 B".into(),
        confirm_label: "Delete permanently".into(),
        danger: true,
        typed: Some("DELETE".into()),
    }
}

fn job(title: &str, state: JobRowState, done: u64, total: u64) -> JobRowProps {
    JobRowProps {
        title: title.to_owned().into(),
        current: Some(r"\\nas\media\photos\2026-10 Sinaia\IMG_4131.jpg".into()),
        bytes_done: done,
        bytes_total: total,
        files_done: 4,
        files_total: 12,
        state,
    }
}

fn jobs() -> [JobRowProps; 4] {
    [
        job(
            r"Copying 12 files to \\nas\media\photos",
            JobRowState::Running,
            GB * 12 / 10,
            GB * 34 / 10,
        ),
        job(
            r"Moving bios-x670e-3201.zip to D:\backups",
            JobRowState::Done,
            31 * MB,
            31 * MB,
        ),
        job(
            r"Copying films\Arrival (2016).mkv to D:\media",
            JobRowState::Failed(r"Access denied: D:\media\films".into()),
            GB / 2,
            GB * 3,
        ),
        job(
            r"Deleting 3 items from D:\tmp",
            JobRowState::Cancelled,
            0,
            0,
        ),
    ]
}

fn transfers(t: &Theme, cx: &mut Context<OperatePage>) -> Div {
    let rows = jobs().into_iter().enumerate().map(|(index, props)| {
        let page = cx.entity();
        JobRow::new(("job", index), props).on_cancel(move |_, cx| {
            page.update(cx, |this, cx| this.set_last("Job cancelled", cx));
        })
    });
    let strip = div()
        .flex()
        .flex_col()
        .py(t.spacing.xs)
        .border_1()
        .border_color(t.colors.line)
        .rounded(t.radii.md)
        .bg(t.colors.surface)
        .children(rows);
    section(t, "JOB ROWS: RUNNING, DONE, FAILED, CANCELLED", strip)
}

impl Render for OperatePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let dialogs = div()
            .flex()
            .items_start()
            .gap(t.spacing.xxl * 2.)
            .child(section(
                &t,
                "PERMANENT DELETE",
                self.answer("confirm-permanent", permanent(), cx),
            ))
            .child(section(
                &t,
                "PROTECTED LOCATION",
                self.answer("confirm-protected", protected(), cx),
            ))
            .child(section(
                &t,
                "LAST ANSWER",
                div().text_color(t.colors.muted).child(self.last.clone()),
            ));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(t.spacing.xxl)
            .p(t.spacing.xxl)
            .bg(t.colors.background)
            .child(dialogs)
            .child(transfers(&t, cx))
    }
}
