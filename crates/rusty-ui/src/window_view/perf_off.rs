use super::FerriteWindow;
use crate::activity::ActivityId;
use crate::prelude::*;

impl FerriteWindow {
    pub(super) fn noted_modal(modal: Option<AnyElement>) -> Option<AnyElement> {
        modal
    }

    pub(super) fn perf_frame(&self, _window: &Window) -> Option<()> {
        None
    }

    pub(super) fn perf_on(root: Div, _frame: Option<()>, _cx: &mut Context<Self>) -> Div {
        root
    }

    pub(super) fn stop_recording(&mut self, _cx: &mut Context<Self>) {}

    pub(super) fn retry_perf_save(&mut self, _cx: &mut Context<Self>) {}

    pub(super) fn perf_dismissed(&mut self, _id: ActivityId) {}

    pub(super) fn save_recording_on_close(&mut self, _cx: &mut Context<Self>) {}

    pub(super) fn open_perf_folder(&mut self, _cx: &mut Context<Self>) {}
}
