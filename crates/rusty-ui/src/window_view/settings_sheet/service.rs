use crate::prelude::*;
use crate::settings::ServiceStatus;
use crate::window_view::FerriteWindow;
use crate::window_view::search_hooks::{ServiceAction, ServiceState};

impl FerriteWindow {
    pub(super) fn refresh_index_service(&mut self, cx: &mut Context<Self>) {
        if !self.settings.service_action_pending {
            self.load_index_service(None, cx);
        }
    }

    pub(super) fn install_index_service(&mut self, cx: &mut Context<Self>) {
        if self.settings.service_status.can_install() {
            self.load_index_service(Some(self.settings.search.service.install.clone()), cx);
        }
    }

    pub(super) fn remove_index_service(&mut self, cx: &mut Context<Self>) {
        if self.settings.service_status.can_remove() {
            self.load_index_service(Some(self.settings.search.service.remove.clone()), cx);
        }
    }

    fn load_index_service(&mut self, action: Option<ServiceAction>, cx: &mut Context<Self>) {
        self.settings.service_action_pending = action.is_some();
        self.settings.service_generation += 1;
        self.settings.service_status = ServiceStatus::NotAvailable;
        let generation = self.settings.service_generation;
        let status = self.settings.search.service.status.clone();
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_spawn(async move { query_status(action, status) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.receive_index_service(generation, status, cx);
            });
        })
        .detach();
    }

    fn receive_index_service(
        &mut self,
        generation: u64,
        status: ServiceStatus,
        cx: &mut Context<Self>,
    ) {
        if self.settings.service_generation != generation {
            return;
        }
        self.settings.service_status = status;
        self.settings.service_action_pending = false;
        cx.notify();
    }
}

fn query_status(action: Option<ServiceAction>, status: ServiceState) -> ServiceStatus {
    if let Some(action) = action {
        action();
    }
    status()
}
