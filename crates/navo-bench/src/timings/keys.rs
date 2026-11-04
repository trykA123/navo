use crate::probe::{Key, Letter};

/// Plain letters typed into the file list; no command is bound to any of them there.
pub(super) const KEY_TEXT: &str = "sens";

pub(super) fn keys() -> Vec<Key> {
    KEY_TEXT
        .chars()
        .filter_map(Letter::new)
        .map(Key::Letter)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYMAP: &str = include_str!("../../../navo-ui/src/commands/keymap.toml");
    /// Contexts of viewers and trees; the file list is not among them.
    const READ_ONLY_PREFIXES: [&str; 7] = [
        "viewer.", "lister.", "image.", "tree.", "elf.", "map.", "trace.",
    ];
    const WRITE_WORDS: [&str; 10] = [
        "copy", "move", "delete", "rename", "create", "paste", "cut", "trash", "save", "write",
    ];

    fn quoted(line: &str) -> Vec<&str> {
        line.split('"').skip(1).step_by(2).collect()
    }

    /// (command, keys) of every `[[bind]]` block.
    fn binds() -> Vec<(String, Vec<String>)> {
        KEYMAP
            .split("[[bind]]")
            .skip(1)
            .filter_map(|block| {
                let line = |name: &str| block.lines().find(|l| l.trim_start().starts_with(name));
                let command = quoted(line("command")?).first()?.to_string();
                let keys = quoted(line("keys")?)
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                Some((command, keys))
            })
            .collect()
    }

    #[test]
    fn posted_keys_are_three_or_four_plain_lowercase_letters_or_digits() {
        assert!((3..=4).contains(&KEY_TEXT.len()));
        assert!(
            KEY_TEXT
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        );
        assert_eq!(keys().len(), KEY_TEXT.len());
    }

    #[test]
    fn no_posted_key_is_bound_to_a_write_in_the_default_keymap() {
        let binds = binds();
        assert!(binds.len() > 200, "the keymap parse must see the binds");
        for key in KEY_TEXT.chars().map(String::from) {
            for (command, _) in binds.iter().filter(|(_, keys)| keys.contains(&key)) {
                assert!(
                    READ_ONLY_PREFIXES.iter().any(|p| command.starts_with(p)),
                    "{key} is bound outside the viewers: {command}"
                );
                assert!(
                    !WRITE_WORDS.iter().any(|w| command.contains(w)),
                    "{command}"
                );
            }
        }
    }
}
