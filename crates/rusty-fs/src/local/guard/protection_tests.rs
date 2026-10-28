use super::*;

fn at(path: &Path) -> Location {
    Location::new(path.to_str().unwrap()).unwrap()
}

#[test]
fn each_distinct_parent_is_canonicalized_once_per_check() {
    let root = tempfile::tempdir().unwrap();
    let mut cached = MutationLocations::default();
    for folder in ["one", "two", "three"] {
        let parent = root.path().join(folder);
        std::fs::create_dir(&parent).unwrap();
        for file in 0..10 {
            let location = at(&parent.join(format!("file-{file}")));
            assert_eq!(cached.resolve(&location), mutation_location(&location));
        }
    }
    assert_eq!(cached.canonicalize_calls.len(), 3);
    let distinct: std::collections::HashSet<_> = cached.canonicalize_calls.iter().collect();
    assert_eq!(distinct.len(), 3);
}

#[test]
fn missing_parents_share_canonicalized_ancestors() {
    let root = tempfile::tempdir().unwrap();
    let mut cached = MutationLocations::default();
    for folder in ["missing/one", "missing/two", "missing/one/deep"] {
        let location = at(&root.path().join(folder).join("file"));
        assert_eq!(cached.resolve(&location), mutation_location(&location));
    }
    let distinct: std::collections::HashSet<_> = cached.canonicalize_calls.iter().collect();
    assert_eq!(cached.canonicalize_calls.len(), distinct.len());
    assert_eq!(distinct.len(), 5);
}

#[test]
fn reserved_names_are_refused_after_an_ordinary_sibling_warms_the_cache() {
    let root = tempfile::tempdir().unwrap();
    let mut cached = MutationLocations::default();
    cached.resolve(&at(&root.path().join("ordinary"))).unwrap();
    for name in [
        ".navo-retained-ab",
        ".navo-held-1-2",
        ".navo-mutations-runtime",
    ] {
        let location = at(&root.path().join(name));
        assert_eq!(cached.resolve(&location), mutation_location(&location));
        assert_eq!(
            cached.resolve(&location).unwrap_err().kind,
            FsErrorKind::AccessDenied
        );
    }
    assert_eq!(cached.canonicalize_calls.len(), 1);
}

#[cfg(unix)]
#[test]
fn cached_aliases_preserve_refusals_and_do_not_resolve_the_leaf() {
    let root = tempfile::tempdir().unwrap();
    let held = root.path().join(".navo-held-1-2");
    std::fs::create_dir(&held).unwrap();
    std::os::unix::fs::symlink(&held, root.path().join("alias")).unwrap();
    std::os::unix::fs::symlink(root.path().join("missing"), root.path().join("dangling")).unwrap();
    let mut cached = MutationLocations::default();
    for name in ["alias/a", "alias/b", "dangling/a", "dangling/b", "alias"] {
        let location = at(&root.path().join(name));
        assert_eq!(cached.resolve(&location), mutation_location(&location));
        assert_eq!(cached.resolve(&location).is_ok(), name == "alias");
    }
}

fn reserved_samples() -> Vec<Location> {
    let roots = ["/", "fake://", r"C:\", r"\\srv\share\", r"\\?\C:\"];
    let parents = [
        "docs",
        ".NAVO-RETAINED-ab",
        "navo-retained-ab",
        ".ferrite-held-ab",
    ];
    let leaves = [
        "new",
        ".navo-held-1-2",
        "notes.navo-held-1",
        ".navo-mutations-x",
    ];
    let mut samples = vec![];
    for root in roots {
        for (parent, leaf) in parents
            .iter()
            .flat_map(|parent| leaves.map(|leaf| (parent, leaf)))
        {
            for suffix in ["", "/", "/deep/file"] {
                samples.push(Location::new(format!("{root}{parent}/{leaf}{suffix}")).unwrap());
            }
        }
    }
    samples
}

#[test]
fn cached_reserved_walk_matches_the_original_for_all_path_forms() {
    let mut cached = MutationLocations::default();
    let samples = reserved_samples();
    for location in samples.iter().rev().chain(&samples) {
        let expected = std::iter::successors(Some(location.clone()), Location::parent)
            .filter_map(|path| path.name())
            .any(|name| rusty_core::brand::is_reserved_name(&name.to_text()));
        assert_eq!(refuse_reserved(location).is_err(), expected, "{location:?}");
        assert_eq!(
            cached.refuse_reserved(location),
            refuse_reserved(location),
            "{location:?}"
        );
    }
}
