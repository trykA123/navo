use super::FerriteWindow;
use crate::prelude::*;

impl FerriteWindow {
    pub(super) fn refresh_diagnostics(&mut self, cx: &mut Context<Self>) {
        self.settings.diagnostics_generation += 1;
        let generation = self.settings.diagnostics_generation;
        let scan = self.settings.disk_use.clone();
        let scanning = cx.background_spawn(async move { scan() });
        cx.spawn(async move |this, cx| {
            let rows = scanning.await;
            let _ = this.update(cx, |view, cx| {
                view.receive_diagnostics(generation, rows, cx);
            });
        })
        .detach();
    }
}

impl FerriteWindow {
    fn receive_diagnostics(
        &mut self,
        generation: u64,
        rows: Vec<crate::settings::DiagnosticRow>,
        cx: &mut Context<Self>,
    ) {
        if self.settings.diagnostics_generation != generation {
            return;
        }
        self.settings.diagnostics = rows;
        cx.notify();
    }
}
