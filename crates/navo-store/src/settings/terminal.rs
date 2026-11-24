//! Terminal settings: the external terminal launcher plus the built-in terminal module's keys.
//!
//! Every key has a serde default, so a file written before the module existed keeps loading.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The built-in terminal's colour scheme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalScheme {
    /// Atom One Dark.
    OneDark,
    /// Catppuccin, Latte or Mocha by theme.
    Catppuccin,
    /// Tokyo Night.
    TokyoNight,
    /// Gruvbox dark.
    Gruvbox,
    /// Dracula.
    Dracula,
    /// Follow the live Navo theme.
    #[default]
    #[serde(other)]
    Navo,
}

/// Where a new terminal session starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalStart {
    /// The active pane's folder.
    #[default]
    Pane,
    /// The user's home folder.
    Home,
}

/// The terminal cursor shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalCursor {
    /// A filled block.
    #[default]
    Block,
    /// A vertical bar.
    Bar,
    /// A line under the cell.
    Underline,
}

/// What the terminal does on a bell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalBell {
    /// Nothing.
    Off,
    /// Mark the session tab.
    #[default]
    Tab,
    /// Play a sound.
    Sound,
}

/// Line height in tenths of a line; the settings file stores it as `1.4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineHeight(u16);

impl LineHeight {
    /// The setting's default: 1.4 lines.
    pub const DEFAULT: Self = Self(14);

    /// The stored tenths of a line.
    pub fn tenths(self) -> u16 {
        self.0
    }
}

impl Default for LineHeight {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Serialize for LineHeight {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f32(f32::from(self.0) / 10.0)
    }
}

impl<'de> Deserialize<'de> for LineHeight {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f32::deserialize(deserializer)?;
        if !(0.8..=2.0).contains(&value) {
            return Err(serde::de::Error::custom(
                "terminal.line-height must be from 0.8 to 2.0",
            ));
        }
        Ok(Self((value * 10.0).round() as u16))
    }
}

/// The terminal settings: the external launcher (provider, command) and the built-in module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Terminal {
    /// The external terminal program: system, Windows Terminal, PowerShell, cmd or custom.
    pub provider: String,
    /// A custom command line; `{path}` is the folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// The built-in shell; an empty string means the first detected shell.
    pub shell: String,
    /// Where a new session starts.
    pub start_in: TerminalStart,
    /// Whether Navo injects OSC 133 prompt marks for the gutter and sync.
    pub prompt_marks: bool,
    /// The grid font family.
    pub font: String,
    /// The grid font size, in points.
    pub font_size: u16,
    /// Line height in tenths of a line; the default 14 is 1.4.
    pub line_height: LineHeight,
    /// The cursor shape.
    pub cursor: TerminalCursor,
    /// Whether the cursor blinks.
    pub blink: bool,
    /// The colour scheme.
    pub scheme: TerminalScheme,
    /// Whether the scheme's colours are lifted to AA contrast.
    pub keep_readable: bool,
    /// Window opacity percentage for the pop-out terminal.
    pub opacity: u16,
    /// Padding around the grid, in points.
    pub padding: u16,
    /// Drawer height percentage of the window.
    pub drawer_height: u16,
    /// Side-panel width, in points.
    pub side_width: u16,
    /// Scrollback ring size, in lines.
    pub scrollback: u32,
    /// Whether selecting text copies it to the clipboard.
    pub copy_on_select: bool,
    /// Whether Ctrl+C copies when text is selected instead of reaching the shell.
    pub ctrl_c_copies: bool,
    /// What a bell does.
    pub bell: TerminalBell,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            provider: "system".to_owned(),
            command: None,
            shell: String::new(),
            start_in: TerminalStart::default(),
            prompt_marks: true,
            font: "JetBrains Mono".to_owned(),
            font_size: 13,
            line_height: LineHeight::default(),
            cursor: TerminalCursor::default(),
            blink: true,
            scheme: TerminalScheme::default(),
            keep_readable: true,
            opacity: 100,
            padding: 8,
            drawer_height: 40,
            side_width: 440,
            scrollback: 10_000,
            copy_on_select: false,
            ctrl_c_copies: true,
            bell: TerminalBell::default(),
        }
    }
}
