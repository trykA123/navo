//! The six automotive view toggles as palette command specs.
//!
//! A kind lists the toggles its body can run through [`ViewerKind::commands`], so Ctrl+K
//! offers a toggle only while a viewer that owns it is open. The names, actions and keys are
//! the ones the static registry and `keymap.toml` already carry.
//!
//! [`ViewerKind::commands`]: super::ViewerKind::commands

use crate::commands::CommandSpec;
use crate::window_view::viewer::CONTEXT;

const fn spec(action: &'static str, name: &'static str, label: &'static str) -> CommandSpec {
    CommandSpec {
        name,
        label,
        context: CONTEXT,
        action: Some(action),
    }
}

const INSPECTOR: CommandSpec = spec(
    "viewer::ToggleOpensInspector",
    "view.opens_inspector",
    "cmd-view-opens-inspector",
);
const MEMORY_STRIP: CommandSpec = spec(
    "viewer::ToggleMemoryStrip",
    "view.memory_strip",
    "cmd-view-memory-strip",
);
const CHARTS: CommandSpec = spec("viewer::ToggleCharts", "view.charts", "cmd-view-charts");
const ROW_BARS: CommandSpec = spec(
    "viewer::ToggleRowBars",
    "view.row_bars",
    "cmd-view-row-bars",
);
const USAGE_BARS: CommandSpec = spec(
    "viewer::ToggleUsageBars",
    "view.usage_bars",
    "cmd-view-usage-bars",
);
const WHO_USES: CommandSpec = spec(
    "viewer::ToggleWhoUses",
    "view.who_uses",
    "cmd-view-who-uses",
);

/// The toggles of a layout viewer: linker map and ELF.
pub(super) const LAYOUT: &[CommandSpec] =
    &[INSPECTOR, MEMORY_STRIP, USAGE_BARS, WHO_USES, ROW_BARS];

/// The toggles of a memory image viewer.
pub(super) const MEMORY: &[CommandSpec] = &[INSPECTOR, MEMORY_STRIP, CHARTS, ROW_BARS];

/// The toggles of a table or outline viewer: its inspector.
pub(super) const INSPECTOR_ONLY: &[CommandSpec] = &[INSPECTOR];
