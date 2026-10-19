use std::rc::Rc;

use gpui_kit::{DisplayId, Global, point};
use rusty_core::placement::{
    Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect, Placement, restore,
};

use super::FerriteWindow;
use crate::prelude::*;

pub type DisplayScale = Rc<dyn Fn(u64) -> Option<f32>>;

pub(super) struct DisplayScales(pub(super) DisplayScale);

impl Global for DisplayScales {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Screen {
    pub(super) id: DisplayId,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) work_area: Bounds<Pixels>,
    pub(super) scale: f32,
    pub(super) primary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Shown {
    pub(super) bounds: WindowBounds,
    pub(super) display: Option<DisplayId>,
}

pub(super) fn screens(cx: &App) -> Vec<Screen> {
    let scale = cx
        .try_global::<DisplayScales>()
        .map(|scales| scales.0.clone());
    let primary = cx.primary_display().map(|display| display.id());
    cx.displays()
        .iter()
        .map(|display| Screen {
            id: display.id(),
            bounds: display.bounds(),
            work_area: display.visible_bounds(),
            scale: scale
                .as_ref()
                .and_then(|scale| scale(display.id().into()))
                .unwrap_or(1.0),
            primary: Some(display.id()) == primary,
        })
        .collect()
}

pub(super) fn opening(
    screens: &[Screen],
    placements: &[Placement],
) -> Option<(DisplayId, WindowBounds)> {
    let setup = setup(screens)?;
    let restored = restore(&setup, placements);
    let (screen, monitor) = screens
        .iter()
        .zip(setup.monitors())
        .find(|(_, monitor)| monitor.bounds == restored.monitor)?;
    let scale = f64::from(screen.scale);
    let logical = |physical: f64| px((physical.round() / scale) as f32);
    let local = restored.bounds;
    let bounds = Bounds {
        origin: point(
            logical(f64::from(monitor.bounds.x) + local.x * scale),
            logical(f64::from(monitor.bounds.y) + local.y * scale),
        ),
        size: size(logical(local.width * scale), logical(local.height * scale)),
    };
    let bounds = if restored.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    };
    Some((screen.id, bounds))
}

pub(super) fn placement(screens: &[Screen], shown: Shown) -> Option<(MonitorSetup, Placement)> {
    let setup = setup(screens)?;
    let (bounds, maximized) = match shown.bounds {
        WindowBounds::Windowed(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, false),
        WindowBounds::Maximized(bounds) => (bounds, true),
    };
    let index = screens
        .iter()
        .position(|screen| Some(screen.id) == shown.display)?;
    let monitor = *setup.monitors().get(index)?;
    let scale = f64::from(monitor.scale);
    let global = |value: Pixels| f64::from(value.as_f32()) * scale;
    let placement = Placement {
        setup: setup.fingerprint(),
        bounds: MonitorLocalLogicalRect {
            x: (global(bounds.origin.x) - f64::from(monitor.bounds.x)) / scale,
            y: (global(bounds.origin.y) - f64::from(monitor.bounds.y)) / scale,
            width: global(bounds.size.width) / scale,
            height: global(bounds.size.height) / scale,
        },
        monitor: monitor.bounds,
        maximized,
    };
    Some((setup, placement))
}

fn setup(screens: &[Screen]) -> Option<MonitorSetup> {
    let monitors = screens.iter().map(monitor).collect::<Option<Vec<_>>>()?;
    MonitorSetup::new(monitors).ok()
}

fn monitor(screen: &Screen) -> Option<Monitor> {
    Some(Monitor {
        bounds: physical(screen.bounds, screen.scale)?,
        work_area: physical(screen.work_area, screen.scale)?,
        scale: screen.scale,
        primary: screen.primary,
    })
}

fn physical(bounds: Bounds<Pixels>, scale: f32) -> Option<PhysicalRect> {
    let whole = |value: Pixels| {
        let value = (f64::from(value.as_f32()) * f64::from(scale)).round();
        (value.is_finite() && value.abs() <= f64::from(i32::MAX)).then_some(value as i64)
    };
    Some(PhysicalRect {
        x: i32::try_from(whole(bounds.origin.x)?).ok()?,
        y: i32::try_from(whole(bounds.origin.y)?).ok()?,
        width: u32::try_from(whole(bounds.size.width)?).ok()?,
        height: u32::try_from(whole(bounds.size.height)?).ok()?,
    })
}

impl FerriteWindow {
    pub(super) fn remember_placement(&mut self, window: &Window, cx: &App) {
        let shown = Shown {
            bounds: window.window_bounds(),
            display: window.display(cx).map(|display| display.id()),
        };
        if let Some((setup, placement)) = placement(&screens(cx), shown) {
            self.browser.remember_placement(&setup, placement);
        }
    }
}

#[cfg(test)]
mod tests;
