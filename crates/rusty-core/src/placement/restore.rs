use super::{Monitor, MonitorLocalLogicalRect, MonitorSetup, Placement, Restored};

#[must_use]
pub fn restore(current: &MonitorSetup, placements: &[Placement]) -> Restored {
    let fingerprint = current.fingerprint();
    if let Some(saved) = placements.iter().find(|saved| {
        saved.setup == fingerprint
            && saved.bounds.is_valid()
            && current.0.iter().any(|m| m.bounds == saved.monitor)
    }) {
        return Restored {
            bounds: saved.bounds,
            monitor: saved.monitor,
            maximized: saved.maximized,
        };
    }
    if let Some(restored) = placements
        .first()
        .and_then(|saved| restore_visible(current, saved))
    {
        return restored;
    }
    let primary = current
        .0
        .iter()
        .find(|m| m.bounds.x == 0 && m.bounds.y == 0)
        .unwrap_or(&current.0[0]);
    let area = logical_work_area(primary);
    let width = 1280.0_f64.min(area.width * 0.8);
    let height = 800.0_f64.min(area.height * 0.8);
    Restored {
        bounds: MonitorLocalLogicalRect {
            x: area.x + (area.width - width) / 2.0,
            y: area.y + (area.height - height) / 2.0,
            width,
            height,
        },
        monitor: primary.bounds,
        maximized: false,
    }
}

struct PhysicalWindowRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl PhysicalWindowRect {
    fn from_placement(saved: &Placement) -> Option<Self> {
        let (_, scale) = saved
            .setup
            .0
            .iter()
            .find(|(bounds, _)| *bounds == saved.monitor)?;
        let scale = f64::from(f32::from_bits(*scale));
        if !saved.bounds.is_valid() || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let bounds = saved.bounds;
        let rect = Self {
            x: f64::from(saved.monitor.x) + bounds.x * scale,
            y: f64::from(saved.monitor.y) + bounds.y * scale,
            width: bounds.width * scale,
            height: bounds.height * scale,
        };
        [
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.x + rect.width,
            rect.y + rect.height,
        ]
        .iter()
        .all(|v| v.is_finite())
        .then_some(rect)
    }

    fn overlap_fraction(&self, monitor: &Monitor) -> f64 {
        let area = monitor.work_area;
        let width = (self.x + self.width).min(f64::from(area.x) + f64::from(area.width))
            - self.x.max(f64::from(area.x));
        let height = (self.y + self.height).min(f64::from(area.y) + f64::from(area.height))
            - self.y.max(f64::from(area.y));
        (width.max(0.0) / self.width) * (height.max(0.0) / self.height)
    }
}

fn restore_visible(current: &MonitorSetup, saved: &Placement) -> Option<Restored> {
    let physical = PhysicalWindowRect::from_placement(saved)?;
    let (monitor, _) = current
        .0
        .iter()
        .map(|m| (m, physical.overlap_fraction(m)))
        .filter(|(_, fraction)| *fraction >= 0.25)
        .max_by(|(_, left), (_, right)| left.total_cmp(right))?;
    let area = logical_work_area(monitor);
    let scale = f64::from(monitor.scale);
    let width = saved.bounds.width.min(area.width);
    let height = saved.bounds.height.min(area.height);
    let x = (physical.x - f64::from(monitor.bounds.x)) / scale;
    let y = (physical.y - f64::from(monitor.bounds.y)) / scale;
    Some(Restored {
        bounds: MonitorLocalLogicalRect {
            x: x.clamp(area.x, area.x + (area.width - width)),
            y: y.clamp(area.y, area.y + (area.height - height)),
            width,
            height,
        },
        monitor: monitor.bounds,
        maximized: saved.maximized,
    })
}

fn logical_work_area(monitor: &Monitor) -> MonitorLocalLogicalRect {
    let area = monitor.work_area;
    let scale = f64::from(monitor.scale);
    MonitorLocalLogicalRect {
        x: (f64::from(area.x) - f64::from(monitor.bounds.x)) / scale,
        y: (f64::from(area.y) - f64::from(monitor.bounds.y)) / scale,
        width: f64::from(area.width) / scale,
        height: f64::from(area.height) / scale,
    }
}
