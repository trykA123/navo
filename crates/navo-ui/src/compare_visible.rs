//! The one switch that hides everything Compare until a compare can be opened.

/// Flipped to true by U4 when a compare can be opened.
const COMPARE_VISIBLE: bool = false;

/// Whether the Compare settings section and the `compare.*` commands and keys are offered.
#[must_use]
pub(crate) fn compare_visible() -> bool {
    #[cfg(test)]
    if let Some(forced) = tests::FORCED.with(std::cell::Cell::get) {
        return forced;
    }
    COMPARE_VISIBLE
}

#[cfg(test)]
pub(crate) use tests::show_compare;

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    thread_local! {
        pub(super) static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    }

    /// Makes Compare visible on this thread until the guard drops.
    pub(crate) struct Shown;

    impl Drop for Shown {
        fn drop(&mut self) {
            FORCED.with(|forced| forced.set(None));
        }
    }

    #[must_use]
    pub(crate) fn show_compare() -> Shown {
        FORCED.with(|forced| forced.set(Some(true)));
        Shown
    }

    #[test]
    fn compare_is_hidden_until_the_flag_flips() {
        assert!(!super::compare_visible());
        let shown = show_compare();
        assert!(super::compare_visible());
        drop(shown);
        assert!(!super::compare_visible());
    }
}
