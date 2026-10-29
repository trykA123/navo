use gpui_kit::SharedString;
use rusty_core::Tiles;
use rusty_fs::CopyMode;

use super::fields::{Control, FIELDS, Field, FieldSpec, PRESETS};
use crate::app_menu::{QuickSettings, Setting};
use crate::browser::DropConfirm;
use crate::file_list::{SortBy, SortKey};
use crate::modifier_hints::HintsMode;
use crate::tokens::{ColorTheme, Density, SyntaxTheme, ThemeChoice};

const THEME_ORDER: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];
const DENSITY_ORDER: [Density; 3] = [Density::Compact, Density::Normal, Density::Comfortable];
const HINTS_ORDER: [HintsMode; 3] = [HintsMode::Off, HintsMode::Fkeys, HintsMode::All];
const SORT_ORDER: [SortBy; 4] = [SortBy::Name, SortBy::Size, SortBy::Date, SortBy::Ext];
const COPY_ORDER: [CopyMode; 2] = [CopyMode::Auto, CopyMode::Standard];
const DROP_CONFIRM_ORDER: [DropConfirm; 3] =
    [DropConfirm::Never, DropConfirm::Moves, DropConfirm::Always];
const SHOWN: usize = 0;
const HIDDEN: usize = 1;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Value {
    Flag(bool),
    Pick(usize),
    Text(SharedString),
    Number(i64),
    Unset,
    Reset,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Change {
    pub field: Field,
    pub value: Value,
}

impl Change {
    pub fn key(&self) -> &'static str {
        self.field.spec().key
    }

    pub fn theme(theme: ThemeChoice) -> Self {
        Self::pick(Field::Theme, &THEME_ORDER, theme)
    }

    pub fn color_theme(theme: ColorTheme) -> Self {
        Self::pick(Field::ColorTheme, &ColorTheme::ALL, theme)
    }

    pub fn density(density: Density) -> Self {
        Self::pick(Field::Density, &DENSITY_ORDER, density)
    }

    pub fn syntax_theme(theme: SyntaxTheme) -> Self {
        Self::pick(Field::TextSyntaxTheme, &SyntaxTheme::ALL, theme)
    }

    pub fn modifier_hints(hints: HintsMode) -> Self {
        Self::pick(Field::ModifierHints, &HINTS_ORDER, hints)
    }

    pub fn show_hidden(shown: bool) -> Self {
        Self {
            field: Field::HiddenFiles,
            value: Value::Pick(if shown { SHOWN } else { HIDDEN }),
        }
    }

    pub fn reset(field: Field) -> Self {
        Self {
            field,
            value: Value::Reset,
        }
    }

    pub fn text(field: Field, text: impl Into<SharedString>) -> Self {
        Self {
            field,
            value: Value::Text(text.into()),
        }
    }

    fn pick<T: PartialEq>(field: Field, order: &[T], wanted: T) -> Self {
        let index = order.iter().position(|item| *item == wanted).unwrap_or(0);
        Self {
            field,
            value: Value::Pick(index),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Values {
    values: Vec<Value>,
    changed: Vec<bool>,
    layouts: Vec<(SharedString, Tiles)>,
}

fn default_of(spec: &FieldSpec) -> Value {
    let text = |text: &'static str| Value::Text(SharedString::new_static(text));
    match spec.field {
        Field::Density => Value::Pick(1),
        Field::LightTheme => text("Navo Light"),
        Field::DarkTheme => text("Navo Dark"),
        Field::Font => text("Inter"),
        Field::FontSize => Value::Number(13),
        Field::RetentionDays => Value::Number(3),
        Field::RetentionCapMib => Value::Number(1024),
        Field::RetentionMinFreeMib => Value::Number(256),
        Field::UiScale => Value::Number(100),
        Field::ModifierHints => Value::Pick(2),
        Field::ModifierHintsDelay => Value::Number(400),
        Field::FoldersFirst
        | Field::SpaceMeasuresFolders
        | Field::TextLineNumbers
        | Field::MarkdownPreviewSplit
        | Field::PreviewTypeBars
        | Field::PreviewLargestFiles => Value::Flag(true),
        Field::SearchRemember | Field::RetainDeletes | Field::RetainOverwrites => Value::Flag(true),
        Field::SearchExclude => text("$Recycle.Bin, System Volume Information"),
        Field::SummonKey => text("Win+Alt+N"),
        Field::WindowFkeyBar => Value::Flag(false),
        field if field.is_window_part() => Value::Flag(true),
        _ => match spec.control {
            Control::Choices(..) => Value::Pick(0),
            Control::Toggle => Value::Flag(false),
            Control::Text => text(""),
            Control::Number => Value::Number(0),
        },
    }
}

impl Default for Values {
    fn default() -> Self {
        Self::from_fn(default_of)
    }
}

impl Values {
    pub fn from_fn(mut value_of: impl FnMut(&FieldSpec) -> Value) -> Self {
        let values: Vec<Value> = FIELDS.iter().map(&mut value_of).collect();
        let changed = FIELDS
            .iter()
            .zip(&values)
            .map(|(spec, value)| *value != default_of(spec))
            .collect();
        Self {
            values,
            changed,
            layouts: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_layout_presets<'a>(
        mut self,
        presets: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        self.layouts = presets
            .into_iter()
            .filter_map(|(name, tiles)| match Tiles::parse(tiles) {
                Ok(tiles) => Some((SharedString::from(name.to_owned()), tiles)),
                Err(error) => {
                    tracing::warn!(preset = name, %error, "layout preset skipped");
                    None
                }
            })
            .collect();
        self
    }

    pub fn layout_presets(&self) -> &[(SharedString, Tiles)] {
        &self.layouts
    }

    #[must_use]
    pub fn marked(mut self, is_users: impl Fn(&FieldSpec) -> bool) -> Self {
        self.changed = FIELDS.iter().map(is_users).collect();
        self
    }

    pub fn is_changed(&self, field: Field) -> bool {
        self.changed[field.index()]
    }

    pub fn changed_count(&self) -> usize {
        self.changed
            .iter()
            .filter(|is_changed| **is_changed)
            .count()
    }

    pub fn get(&self, field: Field) -> &Value {
        &self.values[field.index()]
    }

    #[must_use]
    pub fn with(&self, change: &Change) -> Self {
        let mut values = self.clone();
        let default = default_of(change.field.spec());
        let value = match &change.value {
            Value::Unset | Value::Reset => default.clone(),
            value => value.clone(),
        };
        values.changed[change.field.index()] = value != default;
        values.values[change.field.index()] = value;
        values
    }

    pub fn change_of(&self, field: Field) -> Change {
        Change {
            field,
            value: self.get(field).clone(),
        }
    }

    fn pick(&self, field: Field) -> usize {
        match self.get(field) {
            Value::Pick(index) => *index,
            _ => 0,
        }
    }

    pub fn theme(&self) -> ThemeChoice {
        THEME_ORDER
            .get(self.pick(Field::Theme))
            .copied()
            .unwrap_or_default()
    }

    pub fn color_theme(&self) -> ColorTheme {
        ColorTheme::ALL
            .get(self.pick(Field::ColorTheme))
            .copied()
            .unwrap_or_default()
    }

    pub fn density(&self) -> Density {
        DENSITY_ORDER
            .get(self.pick(Field::Density))
            .copied()
            .unwrap_or_default()
    }

    pub fn syntax_theme(&self) -> SyntaxTheme {
        SyntaxTheme::ALL
            .get(self.pick(Field::TextSyntaxTheme))
            .copied()
            .unwrap_or_default()
    }

    pub fn modifier_hints(&self) -> HintsMode {
        HINTS_ORDER
            .get(self.pick(Field::ModifierHints))
            .copied()
            .unwrap_or_default()
    }

    pub fn modifier_hints_delay_ms(&self) -> i64 {
        match self.get(Field::ModifierHintsDelay) {
            Value::Number(ms) => *ms,
            _ => 400,
        }
    }

    pub fn drop_confirm(&self) -> DropConfirm {
        DROP_CONFIRM_ORDER
            .get(self.pick(Field::DropConfirm))
            .copied()
            .unwrap_or_default()
    }

    pub fn show_hidden(&self) -> bool {
        self.pick(Field::HiddenFiles) == SHOWN
    }

    pub fn flag(&self, field: Field) -> bool {
        matches!(self.get(field), Value::Flag(true))
    }

    pub fn show_system(&self) -> bool {
        self.flag(Field::ShowSystem)
    }

    pub fn keeps_in_tray(&self) -> bool {
        self.flag(Field::KeepInTray)
    }

    pub fn summon_key(&self) -> &str {
        match self.get(Field::SummonKey) {
            Value::Text(text) => text,
            _ => "",
        }
    }

    pub fn folders_first(&self) -> bool {
        self.flag(Field::FoldersFirst)
    }

    pub fn sort_key(&self) -> SortKey {
        SortKey {
            by: SORT_ORDER
                .get(self.pick(Field::Sort))
                .copied()
                .unwrap_or_default(),
            descending: self.flag(Field::SortDescending),
        }
    }

    pub fn space_measures_folders(&self) -> bool {
        self.flag(Field::SpaceMeasuresFolders)
    }

    pub fn patterns_select_folders(&self) -> bool {
        self.flag(Field::PatternsSelectFolders)
    }

    pub fn copy_mode(&self) -> CopyMode {
        COPY_ORDER
            .get(self.pick(Field::CopyMode))
            .copied()
            .unwrap_or_default()
    }

    pub fn terminal_provider(&self) -> &'static str {
        let field = Field::TerminalProvider;
        match (self.get(field), field.spec().control) {
            (Value::Pick(at), Control::Choices(options, _)) => {
                options.get(*at).map_or("system", |option| option.toml)
            }
            _ => "system",
        }
    }

    pub fn editor_path(&self) -> Option<String> {
        match self.get(Field::EditorPath) {
            Value::Text(text) if !text.is_empty() => Some(text.to_string()),
            _ => None,
        }
    }

    pub fn is_shown(&self, preset: Option<Field>, part: Field) -> bool {
        let Some(preset) = preset else {
            return self.flag(part);
        };
        match PRESETS.get(self.pick(preset)) {
            Some(preset) if preset.toml == "rich" => true,
            Some(preset) if preset.toml == "custom" => self.flag(part),
            _ => false,
        }
    }

    pub fn opens_inspector(&self) -> bool {
        self.is_shown(Some(Field::AutomotivePreset), Field::AutomotiveInspector)
    }

    pub fn module_prefixes(&self) -> Vec<String> {
        match self.get(Field::AutomotiveModules) {
            Value::Text(text) => text
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn quick(&self) -> QuickSettings {
        QuickSettings {
            theme: self.theme(),
            density: self.density(),
            show_hidden: self.show_hidden(),
        }
    }

    pub fn toggled(&self, setting: Setting) -> Change {
        let toggled = self.quick().toggled(setting);
        match setting {
            Setting::Theme => Change::theme(toggled.theme),
            Setting::Density => Change::density(toggled.density),
            Setting::HiddenFiles => Change::show_hidden(toggled.show_hidden),
        }
    }
}
