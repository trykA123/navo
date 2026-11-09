//! Settings actions that install, remove or refresh the index service.

use crate::prelude::*;
use crate::settings::ServiceStatus;
use crate::window_view::NavoWindow;
use crate::window_view::search_hooks::{ServiceChange, ServiceOutcome, ServiceState};

impl NavoWindow {
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

    pub(super) fn load_index_service(
        &mut self,
        action: Option<ServiceChange>,
        cx: &mut Context<Self>,
    ) {
        self.settings.service_action_pending = action.is_some();
        self.settings.service_generation += 1;
        self.settings.service_status = ServiceStatus::NotAvailable;
        self.settings.service_notice = None;
        let generation = self.settings.service_generation;
        let status = self.settings.search.service.status.clone();
        cx.spawn(async move |this, cx| {
            let (status, outcome) = cx
                .background_spawn(async move { query_status(action, status) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.receive_index_service(generation, (status, outcome), cx);
            });
        })
        .detach();
    }

    fn receive_index_service(
        &mut self,
        generation: u64,
        (status, outcome): (ServiceStatus, ServiceOutcome),
        cx: &mut Context<Self>,
    ) {
        if self.settings.service_generation != generation {
            return;
        }
        let key = match outcome {
            ServiceOutcome::Done => None,
            ServiceOutcome::Deferred => Some("settings-search-service-deferred"),
            ServiceOutcome::Failed => Some("settings-search-service-failed"),
            ServiceOutcome::Locked => Some("settings-search-service-locked"),
        };
        if let Some(key) = key {
            let message: SharedString = navo_l10n::tr(key).into();
            if outcome == ServiceOutcome::Deferred {
                self.settings.service_notice = Some(message);
            } else {
                self.settings.notice = Some(message);
            }
        }
        self.settings.service_status = status;
        self.settings.service_action_pending = false;
        cx.notify();
    }
}

fn query_status(
    action: Option<ServiceChange>,
    status: ServiceState,
) -> (ServiceStatus, ServiceOutcome) {
    let outcome = action.map_or(ServiceOutcome::Done, |action| action());
    (status(), outcome)
}
