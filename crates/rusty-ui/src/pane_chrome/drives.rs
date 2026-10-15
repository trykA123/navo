use gpui_kit::SharedString;
use rusty_core::Location;

use crate::format;
use crate::primitives::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveIcon {
    Fixed,
    Removable,
    Network,
    Optical,
    Ram,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriveInfo {
    pub root: Location,
    pub letter: Option<char>,
    pub icon: DriveIcon,
    pub label: Option<String>,
    pub remote: Option<String>,
    pub connected: bool,
    pub space: Option<(u64, u64)>,
}

const NEARLY_FULL: f32 = 0.9;

impl DriveIcon {
    pub(crate) fn glyph(self) -> IconName {
        match self {
            Self::Fixed | Self::Unknown => IconName::HardDrive,
            Self::Removable => IconName::Usb,
            Self::Network => IconName::Network,
            Self::Optical => IconName::Disc,
            Self::Ram => IconName::MemoryStick,
        }
    }

    pub(crate) fn kind(self) -> &'static str {
        match self {
            Self::Fixed => "Local disk",
            Self::Removable => "Removable drive",
            Self::Network => "Network drive",
            Self::Optical => "Optical drive",
            Self::Ram => "RAM disk",
            Self::Unknown => "Drive",
        }
    }
}

impl DriveInfo {
    #[must_use]
    pub fn name(&self) -> SharedString {
        match (&self.letter, &self.label) {
            (Some(letter), _) => letter.to_string().into(),
            (None, Some(label)) => label.clone().into(),
            (None, None) => self.root.path().display().into_owned().into(),
        }
    }

    #[must_use]
    pub fn used(&self) -> Option<f32> {
        let (total, free) = self.space.filter(|(total, _)| *total > 0)?;
        let used = total.saturating_sub(free) as f64 / total as f64;
        Some(used.clamp(0., 1.) as f32)
    }

    #[must_use]
    pub fn is_nearly_full(&self) -> bool {
        self.used().is_some_and(|used| used > NEARLY_FULL)
    }

    #[must_use]
    pub fn free_text(&self) -> Option<String> {
        let (total, free) = self.space?;
        Some(format!(
            "{} free of {}",
            format::size(free),
            format::size(total)
        ))
    }

    #[must_use]
    pub fn status(&self) -> String {
        if self.connected {
            return self.free_text().unwrap_or_default();
        }
        match &self.remote {
            Some(remote) => format!("Disconnected · {remote}"),
            None => "Disconnected".to_owned(),
        }
    }

    #[must_use]
    pub fn tooltip(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        parts.extend(self.label.clone());
        parts.push(self.icon.kind().to_owned());
        parts.extend(self.free_text());
        parts.extend(self.remote.clone());
        if !self.connected {
            parts.push("Disconnected".to_owned());
        }
        parts.join(" · ")
    }
}

#[must_use]
pub fn current_drive(drives: &[DriveInfo], location: &Location) -> Option<usize> {
    let root = location.root();
    drives.iter().position(|drive| root.same_place(&drive.root))
}
