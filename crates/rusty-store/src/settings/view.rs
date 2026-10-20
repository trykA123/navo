use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewPreset {
    #[default]
    Simple,
    Rich,
    Custom,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TextViewer {
    pub view: TextView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TextView {
    pub line_numbers: bool,
    pub line_ends: bool,
    pub syntax_theme: SyntaxThemeChoice,
}

impl Default for TextView {
    fn default() -> Self {
        Self {
            line_numbers: true,
            line_ends: false,
            syntax_theme: SyntaxThemeChoice::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyntaxThemeChoice {
    #[default]
    OxidisedIron,
    Mocha,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ImageViewer {
    pub view: ImageView,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ImageView {
    pub details: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MarkdownViewer {
    pub view: MarkdownView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MarkdownView {
    pub preview_split: bool,
}

impl Default for MarkdownView {
    fn default() -> Self {
        Self {
            preview_split: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PreviewViewer {
    pub view: PreviewView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PreviewView {
    pub type_bars: bool,
    pub largest_files: bool,
}

impl Default for PreviewView {
    fn default() -> Self {
        Self {
            type_bars: true,
            largest_files: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Automotive {
    pub view: AutomotiveView,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AutomotiveView {
    pub preset: ViewPreset,
    pub inspector: bool,
    pub memory_strip: bool,
    pub charts: bool,
    pub row_bars: bool,
    pub usage_bars: bool,
    pub who_uses: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WindowSettings {
    pub view: WindowView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WindowView {
    pub tabs_with_one: bool,
    pub path_bar: bool,
    pub drive_button: bool,
    pub title_search: bool,
    pub fkey_bar: bool,
    pub icons: bool,
    pub ext_column: bool,
    pub size_column: bool,
    pub date_column: bool,
    pub free_space: bool,
    pub marked_totals: bool,
    pub contents: bool,
}

impl Default for WindowView {
    fn default() -> Self {
        Self {
            tabs_with_one: true,
            path_bar: true,
            drive_button: true,
            title_search: true,
            fkey_bar: false,
            icons: true,
            ext_column: true,
            size_column: true,
            date_column: true,
            free_space: true,
            marked_totals: true,
            contents: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CopySettings {
    pub mode: CopyMode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CopyMode {
    #[default]
    Auto,
    Standard,
}
