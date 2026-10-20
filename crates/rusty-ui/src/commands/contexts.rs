pub const GLOBAL: &str = "global";

const VIEWER: &str = "Viewer";

const PARENTS: [(&str, &str); 20] = [
    ("Archive", VIEWER),
    ("Elf", VIEWER),
    ("ElfTable", "Elf"),
    ("CanTrace", VIEWER),
    ("FindAll", VIEWER),
    ("Image", VIEWER),
    ("Map", VIEWER),
    ("MapTable", "Map"),
    ("Markdown", VIEWER),
    ("MarkdownPreview", VIEWER),
    ("MemoryImage", VIEWER),
    ("Paged", VIEWER),
    ("Pdf", VIEWER),
    ("Table", VIEWER),
    ("Tree", VIEWER),
    ("ViewerFind", VIEWER),
    ("ViewerGoTo", VIEWER),
    ("DialogButton", "Dialog"),
    ("SettingsField", "SettingsSheet"),
    ("SettingsFind", "SettingsSheet"),
];

fn parent(context: &str) -> Option<&str> {
    if let Some((outer, _)) = context.rsplit_once(" > ") {
        return Some(outer);
    }
    PARENTS
        .iter()
        .find(|(child, _)| *child == context)
        .map(|(_, parent)| *parent)
}

#[must_use]
pub fn is_within(context: &str, home: &str) -> bool {
    let mut at = Some(context);
    while let Some(current) = at {
        if current == home {
            return true;
        }
        at = parent(current);
    }
    false
}
