//! The one switch for everything Compare shows: the Settings section, commands and keys.

/// True since U5: a compare can be opened and its rows are drawn.
const COMPARE_VISIBLE: bool = true;

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
pub(crate) use tests::{hide_compare, show_compare};

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    thread_local! {
        pub(super) static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    }

    /// Forces Compare on or off on this thread until the guard drops.
    pub(crate) struct Forced;

    impl Drop for Forced {
        fn drop(&mut self) {
            FORCED.with(|forced| forced.set(None));
        }
    }

    #[must_use]
    pub(crate) fn show_compare() -> Forced {
        FORCED.with(|forced| forced.set(Some(true)));
        Forced
    }

    /// For the tests of the off state.
    #[must_use]
    pub(crate) fn hide_compare() -> Forced {
        FORCED.with(|forced| forced.set(Some(false)));
        Forced
    }

    #[test]
    fn compare_is_visible_and_the_seam_can_force_it_off() {
        assert!(super::compare_visible());
        let hidden = hide_compare();
        assert!(!super::compare_visible());
        drop(hidden);
        assert!(super::compare_visible());
    }
}
