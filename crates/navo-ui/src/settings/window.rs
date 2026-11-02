use super::fields::Field;
use super::values::Values;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowParts {
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

impl Default for WindowParts {
    fn default() -> Self {
        let mut parts = Self::all(true);
        parts.fkey_bar = false;
        parts
    }
}

impl WindowParts {
    #[must_use]
    pub fn all(is_shown: bool) -> Self {
        Self {
            tabs_with_one: is_shown,
            path_bar: is_shown,
            drive_button: is_shown,
            title_search: is_shown,
            fkey_bar: is_shown,
            icons: is_shown,
            ext_column: is_shown,
            size_column: is_shown,
            date_column: is_shown,
            free_space: is_shown,
            marked_totals: is_shown,
            contents: is_shown,
        }
    }

    #[must_use]
    pub fn of(values: &Values) -> Self {
        Self {
            tabs_with_one: values.flag(Field::WindowTabsWithOne),
            path_bar: values.flag(Field::WindowPathBar),
            drive_button: values.flag(Field::WindowDriveButton),
            title_search: values.flag(Field::WindowTitleSearch),
            fkey_bar: values.flag(Field::WindowFkeyBar),
            icons: values.flag(Field::WindowIcons),
            ext_column: values.flag(Field::WindowExtColumn),
            size_column: values.flag(Field::WindowSizeColumn),
            date_column: values.flag(Field::WindowDateColumn),
            free_space: values.flag(Field::WindowFreeSpace),
            marked_totals: values.flag(Field::WindowMarkedTotals),
            contents: values.flag(Field::WindowContents),
        }
    }
}
