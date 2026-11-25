use std::rc::Rc;

use gpui_kit::{DisplayId, Global, point};
use navo_core::placement::{
    Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect, Placement, Restored, restore,
    restore_saved,
};

use super::NavoWindow;
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
    restored_to_window(screens, &setup, restored)
}

/// Restores a saved window placement onto one of the current screens, returning
/// `None` when no saved record applies.
pub(super) fn opening_saved(
    screens: &[Screen],
    placements: &[Placement],
) -> Option<(DisplayId, WindowBounds)> {
    let setup = setup(screens)?;
    let restored = restore_saved(&setup, placements)?;
    restored_to_window(screens, &setup, restored)
}

/// Computes the initial placement for a secondary window centred on `navo`
/// inside `screen.work_area` (up to 1040×800, inset 48 px per side, min 360×420).
pub(super) fn first_place(navo: Bounds<Pixels>, screen: &Screen) -> WindowBounds {
    const SETTINGS_DEFAULT_W: f32 = 1040.0;
    const SETTINGS_DEFAULT_H: f32 = 800.0;
    const SETTINGS_MARGIN: f32 = 48.0;
    const SETTINGS_MIN_W: f32 = 360.0;
    const SETTINGS_MIN_H: f32 = 420.0;

    let area = screen.work_area;
    let width = px(SETTINGS_DEFAULT_W)
        .min(area.size.width - px(SETTINGS_MARGIN * 2.0))
        .max(px(SETTINGS_MIN_W));
    let height = px(SETTINGS_DEFAULT_H)
        .min(area.size.height - px(SETTINGS_MARGIN * 2.0))
        .max(px(SETTINGS_MIN_H));
    let half = |gap: Pixels| px(gap.as_f32() * 0.5);
    let origin = point(
        navo.origin.x + half(navo.size.width - width),
        navo.origin.y + half(navo.size.height - height),
    );
    fit_on(
        WindowBounds::Windowed(Bounds {
            origin,
            size: size(width, height),
        }),
        area,
    )
}

/// Clamps `bounds` to fit inside `area`'s dimensions, then shifts it on-screen.
pub(super) fn fit_on(mut bounds: WindowBounds, area: Bounds<Pixels>) -> WindowBounds {
    let (WindowBounds::Windowed(rect)
    | WindowBounds::Maximized(rect)
    | WindowBounds::Fullscreen(rect)) = &mut bounds;
    rect.size.width = rect.size.width.min(area.size.width);
    rect.size.height = rect.size.height.min(area.size.height);
    keep_on(bounds, area)
}

// Pins the seam shapes until start.rs wires these helpers; the anchors also keep
// them off the non-test `dead_code` list without a lint exception.
type OnScreen = Option<(DisplayId, WindowBounds)>;
const _: fn(&[Screen], &[Placement]) -> OnScreen = opening_saved;
const _: fn(Bounds<Pixels>, &Screen) -> WindowBounds = first_place;

/// Maps a restored logical placement onto the screen that owns its monitor.
fn restored_to_window(
    screens: &[Screen],
    setup: &MonitorSetup,
    restored: Restored,
) -> Option<(DisplayId, WindowBounds)> {
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

pub(super) fn keep_on(mut bounds: WindowBounds, area: Bounds<Pixels>) -> WindowBounds {
    let (WindowBounds::Windowed(rect)
    | WindowBounds::Maximized(rect)
    | WindowBounds::Fullscreen(rect)) = &mut bounds;
    rect.origin.x = rect
        .origin
        .x
        .min(area.right() - rect.size.width)
        .max(area.left());
    rect.origin.y = rect
        .origin
        .y
        .min(area.bottom() - rect.size.height)
        .max(area.top());
    bounds
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

impl NavoWindow {
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
