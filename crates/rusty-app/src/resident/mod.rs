#[cfg(windows)]
mod window;

use rusty_ui::window_view::WindowShown;

#[cfg(windows)]
pub(crate) fn window_shown() -> Option<WindowShown> {
    Some(std::sync::Arc::new(window::set_shown))
}

#[cfg(not(windows))]
pub(crate) fn window_shown() -> Option<WindowShown> {
    None
}

#[cfg(test)]
mod tests {
    use super::window_shown;

    #[test]
    fn only_windows_gets_the_hide_hook() {
        assert_eq!(window_shown().is_some(), cfg!(windows));
    }

    #[test]
    fn a_window_without_a_handle_is_neither_hidden_nor_shown() {
        if let Some(hook) = window_shown() {
            assert!(!hook(None, false));
            assert!(!hook(None, true));
        }
    }
}
