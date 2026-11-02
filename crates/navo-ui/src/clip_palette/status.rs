//! The status line: what the last action did, and the countdown after a secret copy.

use std::time::Duration;

use gpui_kit::{AsyncApp, Context, SharedString, WeakEntity};
use navo_l10n::tr;

use super::view::ClipPalette;

const STATUS_FOR: Duration = Duration::from_millis(2600);
const TICK: Duration = Duration::from_secs(1);
/// How long the service keeps a secret on the clipboard. The countdown only shows it.
const CLEAR_AFTER: u32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Done,
    Error,
    Secret,
}

/// The status line shown for a moment after an action. `left` is the seconds still to count.
pub(super) struct Status {
    pub text: SharedString,
    pub tone: Tone,
    pub serial: u64,
    pub left: Option<u32>,
}

/// Ticks once a second until the countdown ends; false when it was replaced or the palette is gone.
async fn count_down(this: &WeakEntity<ClipPalette>, serial: u64, cx: &mut AsyncApp) -> bool {
    for left in (0..CLEAR_AFTER).rev() {
        cx.background_executor().timer(TICK).await;
        let is_current = this.update(cx, |palette, cx| palette.tick(serial, left, cx));
        if !is_current.unwrap_or(false) {
            return false;
        }
    }
    true
}

impl ClipPalette {
    fn show(&mut self, text: String, tone: Tone, left: Option<u32>) -> u64 {
        self.overlay.serial += 1;
        let serial = self.overlay.serial;
        self.overlay.status = Some(Status {
            text: text.into(),
            tone,
            serial,
            left,
        });
        serial
    }

    pub(super) fn say(&mut self, text: String, is_error: bool, cx: &mut Context<Self>) {
        let tone = if is_error { Tone::Error } else { Tone::Done };
        let serial = self.show(text, tone, None);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STATUS_FOR).await;
            let _ = this.update(cx, |palette, cx| palette.forget_status(serial, cx));
        })
        .detach();
        cx.notify();
    }

    /// A secret went to the clipboard: counts down from 30 s, then says it is cleared.
    pub(super) fn say_clearing(&mut self, text: String, cx: &mut Context<Self>) {
        let serial = self.show(text, Tone::Secret, Some(CLEAR_AFTER));
        cx.spawn(async move |this, cx| {
            if !count_down(&this, serial, cx).await {
                return;
            }
            cx.background_executor().timer(STATUS_FOR).await;
            let _ = this.update(cx, |palette, cx| palette.forget_status(serial, cx));
        })
        .detach();
        cx.notify();
    }

    /// False when a newer status has replaced the one that is counting.
    fn tick(&mut self, serial: u64, left: u32, cx: &mut Context<Self>) -> bool {
        let Some(status) = self
            .overlay
            .status
            .as_mut()
            .filter(|status| status.serial == serial)
        else {
            return false;
        };
        if left == 0 {
            status.text = tr("clip-status-cleared").into();
            status.tone = Tone::Done;
        }
        status.left = (left > 0).then_some(left);
        cx.notify();
        true
    }

    fn forget_status(&mut self, serial: u64, cx: &mut Context<Self>) {
        if self
            .overlay
            .status
            .as_ref()
            .is_some_and(|s| s.serial == serial)
        {
            self.overlay.status = None;
            cx.notify();
        }
    }
}
