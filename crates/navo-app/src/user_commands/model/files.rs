//! Whether a command's arguments take files or names from the window.

use super::{Part, Placeholder, Token};

/// Whether `args` use a pane value, so the command accepts files.
pub(crate) fn takes_files(args: &[Token]) -> bool {
    args.iter().any(|token| {
        token.parts.iter().any(|part| {
            matches!(
                part,
                Part::Placeholder(
                    Placeholder::File
                        | Placeholder::Name
                        | Placeholder::Stem
                        | Placeholder::Ext
                        | Placeholder::Marked
                        | Placeholder::MarkedList
                )
            )
        })
    })
}
