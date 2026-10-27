use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::view::{
    Automotive, CopySettings, ImageViewer, MarkdownViewer, PreviewViewer, TextViewer,
    WindowSettings,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub appearance: Appearance,
    pub files: Files,
    pub selection: SelectionSettings,
    pub terminal: Terminal,
    pub editor: Editor,
    pub window: WindowSettings,
    pub layout: super::layout::LayoutSettings,
    pub text: TextViewer,
    pub image: ImageViewer,
    pub markdown: MarkdownViewer,
    pub preview: PreviewViewer,
    pub copy: CopySettings,
    pub retention: super::retention::RetentionSettings,
    pub search: Search,
    pub automotive: Automotive,
    pub experimental: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Appearance {
    pub mode: ThemeMode,
    pub color_theme: ColorTheme,
    #[serde(deserialize_with = "theme_name")]
    pub light_theme: String,
    #[serde(deserialize_with = "theme_name")]
    pub dark_theme: String,
    pub font: String,
    pub font_size: u16,
    #[serde(deserialize_with = "ui_scale")]
    pub ui_scale: u16,
    pub density: Density,
    pub modifier_hints: ModifierHints,
    pub modifier_hints_delay_ms: u16,
}

pub(super) fn navo_theme_name(name: &str) -> Option<String> {
    name.strip_prefix("Ferrite ")
        .map(|rest| format!("Navo {rest}"))
}

fn ui_scale<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    let percent = u16::deserialize(deserializer)?;
    if !(80..=200).contains(&percent) {
        return Err(serde::de::Error::custom(
            "appearance.ui-scale must be from 80 to 200 percent",
        ));
    }
    Ok(percent)
}

fn theme_name<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let name = String::deserialize(deserializer)?;
    Ok(navo_theme_name(&name).unwrap_or(name))
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: ThemeMode::default(),
            color_theme: ColorTheme::default(),
            light_theme: "Navo Light".to_owned(),
            dark_theme: "Navo Dark".to_owned(),
            font: "Inter".to_owned(),
            font_size: 13,
            ui_scale: 100,
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
pub enum ColorTheme {
    RedBill,
    BlackCap,
    FineMesh,
    Grain,
    InkAndAmber,
    #[default]
    #[serde(other)]
    OpenSea,
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
    pub size_mode: SizeMode,
    pub measure_shown_folders: bool,
    pub drop_confirm: DropConfirm,
    pub rename_log: bool,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            hidden_files: HiddenFiles::default(),
            show_system: false,
            folders_first: true,
            sort: SortKey::default(),
            sort_descending: false,
            size_mode: SizeMode::default(),
            measure_shown_folders: false,
            drop_confirm: DropConfirm::default(),
            rename_log: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SizeMode {
    #[default]
    Apparent,
    OnDisk,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DropConfirm {
    #[default]
    Never,
    Moves,
    Always,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Search {
    pub index: SearchIndex,
    pub drives: Vec<String>,
    pub walk_roots: Vec<String>,
    pub exclude: Vec<String>,
    pub network: bool,
    pub removable: bool,
    pub remember: bool,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            index: SearchIndex::default(),
            drives: Vec::new(),
            walk_roots: Vec::new(),
            exclude: vec![
                "$Recycle.Bin".to_owned(),
                "System Volume Information".to_owned(),
            ],
            network: false,
            removable: false,
            remember: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchIndex {
    #[default]
    Auto,
    Service,
    Walk,
    Off,
}
