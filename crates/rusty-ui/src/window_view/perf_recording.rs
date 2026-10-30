use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::base::Root;
use gpui_kit::{AsyncApp, Task, WeakEntity};
use rusty_core::Location;
use rusty_diag::perf::IndexSample;

use super::FerriteWindow;
use super::keys::{RecordPerformance, StopRecording};
use super::path_edit::last_name;
use super::placement::DisplayScales;
use crate::activity::ActivityId;
use crate::command_center::{IndexMode, IndexOptions, Indexing};
use crate::perf::{
    self, FrameStart, GpuFacts, MonitorFacts, ProcessSample, RECORD_FOR, Recording, RecordingId,
    UiFlag, UiState,
};
use crate::prelude::*;
use crate::settings::ServiceStatus;

pub(super) const PERF_ACTIVITY: u64 = 1;
const TICK: Duration = Duration::from_secs(1);

#[derive(Clone, Copy)]
struct Owned {
    id: RecordingId,
    started: Instant,
}

struct Unsaved {
    recording: Recording,
    ended: Instant,
    gpu: Option<GpuFacts>,
    monitors: Vec<MonitorFacts>,
}

impl Unsaved {
    fn save(&self, service: &dyn perf::PerfService) -> Result<PathBuf, String> {
        let mut system = service.system();
        system.gpu.clone_from(&self.gpu);
        system.monitors.clone_from(&self.monitors);
        service.save(&self.recording, self.ended, &system)
    }
}

#[derive(Default)]
pub(super) struct PerfUi {
    owned: Option<Owned>,
    ticker: Option<Task<()>>,
    saved: Option<Location>,
    unsaved: Option<Arc<Unsaved>>,
    is_saving: bool,
}

pub(super) fn clock_text(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn gpu_facts(window: &Window) -> Option<GpuFacts> {
    let specs = window.gpu_specs()?;
    let backend = if cfg!(windows) { "direct3d11" } else { "wgpu" };
    Some(GpuFacts {
        name: specs.device_name,
        driver: specs.driver_name,
        driver_info: specs.driver_info,
        backend: backend.to_owned(),
        is_software: specs.is_software_emulated,
    })
}

fn monitors(cx: &App) -> Vec<MonitorFacts> {
    let scale = cx
        .try_global::<DisplayScales>()
        .map(|scales| scales.0.clone());
    cx.displays()
        .iter()
        .map(|display| {
            let scale = scale.as_ref().and_then(|scale| scale(display.id().into()));
            let size = display.bounds().size;
            let physical =
                |logical: Pixels| (f32::from(logical) * scale.unwrap_or(1.0)).round() as u32;
            MonitorFacts {
                width_px: physical(size.width),
                height_px: physical(size.height),
                refresh_hz: display
                    .refresh_interval()
                    .map(|interval| 1.0 / interval.as_secs_f32()),
                scale,
            }
        })
        .collect()
}

fn service_name(status: ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::NotInstalled => "not_installed",
        ServiceStatus::Running => "running",
        ServiceStatus::Stopped => "stopped",
        ServiceStatus::NotAvailable => "not_available",
    }
}

fn mode_name(mode: IndexMode) -> &'static str {
    match mode {
        IndexMode::Auto => "auto",
        IndexMode::Service => "service",
        IndexMode::Walk => "walk",
        IndexMode::Off => "off",
    }
}

struct Tick {
    sample: ProcessSample,
    indexing: Option<Indexing>,
    service: ServiceStatus,
}

impl FerriteWindow {
    pub(super) fn noted_modal(modal: Option<AnyElement>) -> Option<AnyElement> {
        if modal.is_some() {
            perf::note(UiFlag::Dialog);
        }
        modal
    }

    pub(super) fn perf_frame(&self, window: &Window) -> Option<FrameStart> {
        perf::frame_begin(|| {
            let state = UiState::default().when(self.settings.is_open(), UiFlag::Settings);
            (window.window_handle().window_id().as_u64(), state)
        })
    }

    pub(super) fn perf_on(root: Div, frame: Option<FrameStart>, cx: &mut Context<Self>) -> Div {
        root.on_action(record_in_root)
            .on_action(
                cx.listener(|view, _: &crate::settings::SaveDiagnosticReport, _, cx| {
                    view.save_diagnostic_report(cx)
                }),
            )
            .when(perf::is_recording(), |root| {
                root.on_action(
                    cx.listener(|view, _: &StopRecording, _, cx| view.stop_recording(cx)),
                )
            })
            .children(frame.map(perf::frame_end))
    }

    pub(super) fn record_performance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let perf = &self.job_ui.perf;
        if perf.is_saving || perf.unsaved.is_some() {
            return;
        }
        let Some(id) = perf::start(Instant::now(), gpu_facts(window)) else {
            return;
        };
        if let Some(service) = perf::service() {
            service.start();
        }
        self.job_ui.perf.owned = Some(Owned {
            id,
            started: cx.background_executor().now(),
        });
        self.show_recording(Duration::ZERO, cx);
        self.job_ui.perf.ticker = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TICK).await;
            while Self::perf_tick(id, &this, cx).await {
                cx.background_executor().timer(TICK).await;
            }
        }));
    }

    async fn perf_tick(id: RecordingId, this: &WeakEntity<Self>, cx: &mut AsyncApp) -> bool {
        let service = perf::service();
        let sample = match service {
            Some(service) => cx.background_spawn(async move { service.sample() }).await,
            None => ProcessSample::default(),
        };
        let Ok(hooks) = this.read_with(cx, |view, _| view.command_center.hooks.clone()) else {
            return false;
        };
        let (indexing, service) = cx
            .background_spawn(async move { ((hooks.index.status)(), (hooks.service.status)()) })
            .await;
        let tick = Tick {
            sample,
            indexing,
            service,
        };
        this.update(cx, |view, cx| view.perf_sampled(id, tick, cx))
            .unwrap_or(false)
    }

    fn perf_sampled(&mut self, id: RecordingId, tick: Tick, cx: &mut Context<Self>) -> bool {
        let Some(owned) = self.job_ui.perf.owned.filter(|owned| owned.id == id) else {
            return false;
        };
        if perf::current() != Some(id) {
            self.end_owned_recording(cx);
            return false;
        }
        let sample = tick.sample;
        let index = IndexSample {
            at_ms: 0,
            entries: tick.indexing.map(|indexing| indexing.count),
            mode: mode_name(IndexOptions::of(&self.values).mode),
            service: service_name(tick.service),
        };
        perf::record(|recording, now| {
            let at_ms = recording.at_ms(now);
            recording.process.push(ProcessSample { at_ms, ..sample });
            recording.index.push(IndexSample {
                at_ms: at_ms as u32,
                ..index
            });
        });
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(owned.started);
        if elapsed >= RECORD_FOR {
            self.stop_recording(cx);
            return false;
        }
        self.show_recording(elapsed, cx);
        true
    }

    fn show_recording(&mut self, elapsed: Duration, cx: &mut Context<Self>) {
        let title = rusty_l10n::t!("perf-recording", time = clock_text(elapsed));
        self.job_ui
            .activity
            .set_stoppable(PERF_ACTIVITY, title.into());
        self.refresh_activity(cx);
    }

    fn end_owned_recording(&mut self, cx: &mut Context<Self>) {
        if self.job_ui.perf.owned.take().is_none() {
            return;
        }
        self.job_ui.perf.ticker = None;
        self.job_ui.activity.end_background(PERF_ACTIVITY);
        self.refresh_activity(cx);
    }

    fn stopped(cx: &App) -> Option<Unsaved> {
        let (recording, gpu) = perf::stop()?;
        Some(Unsaved {
            recording,
            ended: Instant::now(),
            gpu,
            monitors: monitors(cx),
        })
    }

    pub(super) fn stop_recording(&mut self, cx: &mut Context<Self>) {
        self.end_owned_recording(cx);
        if let Some(unsaved) = Self::stopped(cx) {
            self.save_recording(Arc::new(unsaved), cx);
        }
    }

    fn save_recording(&mut self, unsaved: Arc<Unsaved>, cx: &mut Context<Self>) {
        self.job_ui.perf.unsaved = Some(unsaved.clone());
        let Some(service) = perf::service() else {
            self.perf_saved(Err(rusty_l10n::tr("perf-no-service")), cx);
            return;
        };
        self.job_ui.perf.is_saving = true;
        let saving = cx.background_spawn(async move { unsaved.save(service.as_ref()) });
        cx.spawn(async move |this, cx| {
            let saved = saving.await;
            let _ = this.update(cx, |view, cx| view.perf_saved(saved, cx));
        })
        .detach();
    }

    pub(super) fn retry_perf_save(&mut self, cx: &mut Context<Self>) {
        if self.job_ui.perf.is_saving {
            return;
        }
        if let Some(unsaved) = self.job_ui.perf.unsaved.clone() {
            self.save_recording(unsaved, cx);
        }
    }

    pub(super) fn perf_dismissed(&mut self, id: ActivityId) {
        if id == ActivityId::Background(PERF_ACTIVITY) {
            self.job_ui.perf.unsaved = None;
            self.job_ui.perf.saved = None;
        }
    }

    fn perf_saved(&mut self, saved: Result<PathBuf, String>, cx: &mut Context<Self>) {
        self.job_ui.perf.is_saving = false;
        match saved {
            Ok(path) => {
                self.job_ui.perf.unsaved = None;
                let shown = path.display().to_string();
                self.job_ui.perf.saved = path.to_str().and_then(|text| Location::new(text).ok());
                let title = rusty_l10n::tr("perf-saved");
                self.job_ui
                    .activity
                    .finish_background(PERF_ACTIVITY, title.into(), shown.into());
            }
            Err(error) if self.job_ui.perf.unsaved.is_some() => {
                let title = rusty_l10n::tr("perf-not-saved");
                self.job_ui
                    .activity
                    .fail_background(PERF_ACTIVITY, title.into(), error.into());
            }
            Err(_) => {}
        }
        self.refresh_activity(cx);
        cx.notify();
    }

    pub(super) fn save_recording_on_close(&mut self, cx: &mut Context<Self>) {
        self.end_owned_recording(cx);
        let stopped = Self::stopped(cx).map(Arc::new);
        let Some(unsaved) = stopped.or_else(|| self.job_ui.perf.unsaved.take()) else {
            return;
        };
        let Some(service) = perf::service() else {
            return;
        };
        if let Err(error) = unsaved.save(service.as_ref()) {
            tracing::error!(%error, "performance recording not saved on close");
        }
    }

    pub(super) fn open_perf_folder(&mut self, cx: &mut Context<Self>) {
        let Some(zip) = self.job_ui.perf.saved.take() else {
            return;
        };
        if let Some(folder) = zip.parent() {
            self.browser.open_tab_at(folder, last_name(&zip));
        }
        self.job_ui.activity.end_background(PERF_ACTIVITY);
        self.refresh_activity(cx);
    }
}

fn record_in_root(_: &RecordPerformance, window: &mut Window, cx: &mut App) {
    let Some(Some(root)) = window.root::<Root>() else {
        return;
    };
    let view = root.read(cx).view().clone();
    if let Ok(view) = view.downcast::<FerriteWindow>() {
        view.update(cx, |view, cx| view.record_performance(window, cx));
    }
}

impl FerriteWindow {
    pub(super) fn save_diagnostic_report(&mut self, cx: &mut Context<Self>) {
        if self.settings.is_saving_report {
            return;
        }
        self.settings.is_saving_report = true;
        let save = self.settings.save_diagnostic_report.clone();
        let saving = cx.background_spawn(async move { save() });
        cx.spawn(async move |this, cx| {
            let result = saving.await;
            let _ = this.update(cx, |view, cx| {
                view.settings.is_saving_report = false;
                let (id, detail, is_error) = match result {
                    Ok(path) => ("diagnostics-saved", path.display().to_string(), false),
                    Err(error) => ("diagnostics-failed", error, true),
                };
                let mut args = rusty_l10n::FluentArgs::new();
                args.set("detail", detail);
                view.show_notice(crate::status_bar::StatusNotice {
                    text: rusty_l10n::tr_args(id, &args).into(),
                    is_error,
                });
                cx.notify();
            });
        })
        .detach();
    }
}
