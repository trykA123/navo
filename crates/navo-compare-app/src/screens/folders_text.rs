//! Text for the side-by-side folder rows: sizes, times, item counts and the width mode.
//! Used by the folders view to fill each cell's meta text (NC132, NC133).

use chrono::{DateTime, Datelike, TimeZone};

/// How much meta text a row cell shows, chosen from the list width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meta {
    /// Size and time.
    Full,
    /// Size only.
    Size,
    /// Names only.
    Bare,
}

/// Formats a byte count in base 1024 with one decimal (`512 B`, `3.1 KB`).
pub fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 1;
    // Round first, so a value that shows as 1024.0 moves up a unit.
    while unit < UNITS.len() - 1 && (value * 10.0).round() / 10.0 >= 1024.0 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Formats a Unix time in ms as `19 Oct 18:22` (this year) or `19 Oct 2024`; empty if out of range.
pub fn time_text<Tz: TimeZone>(ms: i64, now: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let Some(utc) = DateTime::from_timestamp_millis(ms) else {
        return String::new();
    };
    let local = utc.with_timezone(&now.timezone());
    let pattern = if local.year() == now.year() {
        "%-d %b %H:%M"
    } else {
        "%-d %b %Y"
    };
    local.format(pattern).to_string()
}

/// Formats a folder's child count (`1 item`, `N items`).
pub fn items_text(n: u32) -> String {
    if n == 1 {
        "1 item".to_owned()
    } else {
        format!("{n} items")
    }
}

impl Meta {
    /// Picks the meta mode for a list width in px: Full at 860 or more, Size at 600, else Bare.
    pub fn for_width(px: f32) -> Meta {
        if px >= 860.0 {
            Meta::Full
        } else if px >= 600.0 {
            Meta::Size
        } else {
            Meta::Bare
        }
    }
}
