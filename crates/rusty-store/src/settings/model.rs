use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::view::{Automotive, CopySettings, ImageViewer, TextViewer, WindowSettings};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub appearance: Appearance,
    pub files: Files,
    pub selection: SelectionSettings,
    pub terminal: Terminal,
    pub editor: Editor,
    pub window: WindowSettings,
    pub text: TextViewer,
    pub image: ImageViewer,
    pub copy: CopySettings,
    pub automotive: Automotive,
    pub experimental: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Appearance {
    pub mode: ThemeMode,
    pub light_theme: String,
    pub dark_theme: String,
    pub font: String,
    pub font_size: u16,
    pub density: Density,
    pub modifier_hints: ModifierHints,
    pub modifier_hints_delay_ms: u16,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: ThemeMode::default(),
            light_theme: "Ferrite Light".to_owned(),
            dark_theme: "Ferrite Dark".to_owned(),
            font: "Inter".to_owned(),
            font_size: 13,
            density: Density::default(),
            modifier_hints: ModifierHints::default(),
            modifier_hints_delay_ms: 400,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    Compact,
    #[default]
    Normal,
    Comfortable,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModifierHints {
    #[default]
    All,
    Fkeys,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Files {
    pub hidden_files: HiddenFiles,
    pub show_system: bool,
    pub folders_first: bool,
    pub sort: SortKey,
    pub sort_descending: bool,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            hidden_files: HiddenFiles::default(),
            show_system: false,
            folders_first: true,
            sort: SortKey::default(),
            sort_descending: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HiddenFiles {
    #[default]
    Shown,
    Hidden,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Date,
    Kind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SelectionSettings {
    pub space_measures_folders: bool,
    pub patterns_select_folders: bool,
}

impl Default for SelectionSettings {
    fn default() -> Self {
        Self {
            space_measures_folders: true,
            patterns_select_folders: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Terminal {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            provider: "windows-terminal".to_owned(),
            command: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Editor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
