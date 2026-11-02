use super::*;

const SAMPLES: [&str; 14] = [
    "/",
    "/docs/a",
    "/docs/.held-x/deep/file",
    "/docs/held-x/",
    "fake:///docs/.held-x",
    "fake://docs/a",
    r"C:\",
    r"C:\Docs\.HELD-x\a",
    r"c:\docs\held-x",
    r"\\srv\share\.held-x\a",
    r"\\?\C:\docs\a",
    r"\\?\UNC\srv\share\docs",
    "/plane\u{100800}\u{10ffff}/.held-\u{100801}/a",
    "relative/.held-x/a",
];

fn original_name(location: &Location) -> Option<ProviderPath> {
    let parent = location.parent()?;
    let rest = location.as_str().strip_prefix(parent.as_str())?;
    let name = rest.trim_start_matches(['\\', '/']);
    (!name.is_empty()).then(|| ProviderPath::from_text(name))
}

fn held(_: &Location, name: Option<&ProviderPath>) -> bool {
    name.is_some_and(|name| name.to_text().to_ascii_lowercase().starts_with(".held-"))
}

#[test]
fn parent_and_name_match_the_separate_answers() {
    for text in SAMPLES {
        let location = loc(text);
        let mut level = Some(location);
        while let Some(current) = level {
            let (parent, name) = current.parent_and_name();
            assert_eq!(name, original_name(&current), "{current:?}");
            assert_eq!(parent, current.parent(), "{current:?}");
            level = parent;
        }
    }
}

#[test]
fn a_warm_memo_answers_like_a_full_ancestor_walk() {
    let mut memo = AncestorMemo::new(held);
    let root = loc("/docs");
    let mut at_root =
        AncestorMemo::new(|level: &Location, _: Option<&ProviderPath>| level.same_place(&root));
    for _ in 0..2 {
        for text in SAMPLES.iter().rev().chain(&SAMPLES) {
            let location = loc(text);
            let levels: Vec<_> =
                std::iter::successors(Some(location.clone()), Location::parent).collect();
            let expected = levels
                .iter()
                .any(|level| held(level, original_name(level).as_ref()));
            assert_eq!(memo.any(&location), expected, "{text}");
            let expected = levels.iter().any(|level| level.same_place(&root));
            assert_eq!(at_root.any(&location), expected, "{text}");
        }
    }
}

#[test]
fn text_without_escapes_round_trips_unchanged() {
    for text in [
        "plain",
        "ünï 📁",
        "\u{fffff}",
        "a\u{100800}\u{100800}b",
        "\u{100000}",
    ] {
        let path = ProviderPath::from_text(text);
        assert_eq!(ProviderPath::from_text(&path.to_text()), path, "{text:?}");
    }
    assert_eq!(ProviderPath::from_text("ünï 📁").as_str(), Some("ünï 📁"));
}
