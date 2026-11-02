mod restore;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MonitorLocalLogicalRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    pub bounds: PhysicalRect,
    pub work_area: PhysicalRect,
    pub scale: f32,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorSetup(Vec<Monitor>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorSetupFingerprint(Vec<(PhysicalRect, u32)>);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub setup: MonitorSetupFingerprint,
    pub bounds: MonitorLocalLogicalRect,
    pub monitor: PhysicalRect,
    pub maximized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Restored {
    pub bounds: MonitorLocalLogicalRect,
    pub monitor: PhysicalRect,
    pub maximized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("monitor setup needs nonempty monitors, contained work areas and positive finite scales")]
pub struct InvalidMonitorSetup;

impl MonitorSetup {
    pub fn new(monitors: Vec<Monitor>) -> Result<Self, InvalidMonitorSetup> {
        if monitors.is_empty() || monitors.iter().any(|monitor| !monitor.is_valid()) {
            return Err(InvalidMonitorSetup);
        }
        Ok(Self(monitors))
    }

    #[must_use]
    pub fn monitors(&self) -> &[Monitor] {
        &self.0
    }

    #[must_use]
    pub fn fingerprint(&self) -> MonitorSetupFingerprint {
        let mut monitors = self
            .0
            .iter()
            .map(|m| (m.bounds, m.scale.to_bits()))
            .collect::<Vec<_>>();
        monitors.sort_unstable();
        MonitorSetupFingerprint(monitors)
    }
}

impl Monitor {
    fn is_valid(&self) -> bool {
        self.scale.is_finite()
            && self.scale > 0.0
            && self.work_area.width > 0
            && self.work_area.height > 0
            && self.work_area.x >= self.bounds.x
            && self.work_area.y >= self.bounds.y
            && i64::from(self.work_area.x) + i64::from(self.work_area.width)
                <= i64::from(self.bounds.x) + i64::from(self.bounds.width)
            && i64::from(self.work_area.y) + i64::from(self.work_area.height)
                <= i64::from(self.bounds.y) + i64::from(self.bounds.height)
    }
}

impl MonitorLocalLogicalRect {
    fn is_valid(&self) -> bool {
        [
            self.x,
            self.y,
            self.width,
            self.height,
            self.x + self.width,
            self.y + self.height,
        ]
        .iter()
        .all(|value| value.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }
}

pub(crate) const MAX_PLACEMENTS: usize = 8;

pub(crate) fn remember(
    placements: &mut Vec<Placement>,
    setup: &MonitorSetup,
    mut placement: Placement,
) {
    placement.setup = setup.fingerprint();
    placements.retain(|old| old.setup != placement.setup);
    placements.truncate(MAX_PLACEMENTS - 1);
    placements.insert(0, placement);
}

pub(crate) fn deserialize_placements<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Placement>, D::Error> {
    let mut placements = Vec::<Placement>::deserialize(deserializer)?;
    placements.truncate(MAX_PLACEMENTS);
    Ok(placements)
}

pub use restore::restore;

#[cfg(test)]
mod tests;
