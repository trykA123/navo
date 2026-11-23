//! Which Settings fields are listed: retired fields stay hidden but keep their saved value (AU9).

use super::fields::Field;

impl Field {
    /// Says whether Settings lists this field; retired fields keep their saved value but are hidden.
    pub fn is_shown(self) -> bool {
        !matches!(
            self,
            Self::LightTheme
                | Self::DarkTheme
                | Self::AutomotiveMemoryStrip
                | Self::AutomotiveCharts
                | Self::AutomotiveRowBars
                | Self::AutomotiveUsageBars
                | Self::AutomotiveWhoUses
        )
    }
}
