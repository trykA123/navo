//! Licence text the wizard shows on its Licence page, embedded at build time.

/// The MIT licence text, a copy of the repo's root `LICENSE-MIT`.
pub const TEXT: &str = include_str!("../assets/LICENSE.txt");

#[cfg(test)]
mod tests {
    #[test]
    fn licence_text_matches_root_mit_licence() {
        let root = include_str!("../../../LICENSE-MIT");
        assert_eq!(super::TEXT, root);
    }
}
