use super::*;

fn at(path: &Path) -> Location {
    crate::startup::path_location(path).unwrap()
}

fn mixed(root: &Path) -> OperationPlan {
    let mut steps = vec![];
    for folder in [
        "one",
        "two",
        "three",
        "navo-retained-ab",
        "notes.navo-held-1",
    ] {
        let parent = root.join("files").join(folder);
        std::fs::create_dir_all(&parent).unwrap();
        for file in 0..3 {
            steps.push(Step::CopyFile {
                source: at(&root.join("source")),
                target: at(&parent.join(format!("file-{file}"))),
                size: 0,
            });
        }
    }
    OperationPlan {
        kind: OpKind::Copy,
        steps,
        totals: rusty_core::Totals {
            files: 15,
            folders: 0,
            bytes: 0,
        },
        conflicts: vec![],
        risks: vec![],
    }
}

fn compare(plan: &OperationPlan, dirs: &StoreDirs, experimental: bool) -> Result<(), String> {
    let uncached = plan
        .steps
        .iter()
        .flat_map(mutated)
        .map(legacy_location)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.message());
    assert_eq!(mutated_paths(plan), uncached);
    let expected = uncached.and_then(|paths| legacy_guard_paths(&paths, dirs, experimental));
    let cached = guard(plan, dirs, experimental);
    assert_eq!(cached, expected);
    cached
}

fn compare_refusals(plan: &OperationPlan, dirs: &StoreDirs, root: &Path) {
    for experimental in [false, true] {
        for path in [
            dirs.config.join("new"),
            dirs.data.join("new"),
            root.join("files/one/.navo-retained-ab/payload"),
            root.join("files/two/.navo-held-1-2"),
            root.join("files/three/.navo-mutations-runtime/new"),
            root.join("files/one/deep/.FERRITE-HELD-ab/payload/new"),
            root.join("files/one/.navo-held-1-2/deep/new"),
            root.join("files/one/.navo-retained-ab"),
            dirs.data.parent().unwrap().to_path_buf(),
        ] {
            let mut plan = plan.clone();
            plan.steps.push(Step::RemoveFile {
                location: at(&path),
            });
            assert!(compare(&plan, dirs, experimental).is_err());
        }
    }
}

#[test]
fn cached_and_uncached_mixed_plans_preserve_profile_and_reserved_refusals() {
    let (root, mut dirs, _journal) = setup();
    let plan = mixed(root.path());
    for experimental in [false, true] {
        compare(&plan, &dirs, experimental).unwrap();
    }
    compare_refusals(&plan, &dirs, root.path());
    dirs.config = root.path().join("missing/config");
    compare(&plan, &dirs, false).unwrap();
    assert!(compare(&plan, &dirs, true).is_err());
    compare_refusals(&plan, &dirs, root.path());
}

fn legacy_guard_paths(
    paths: &[Location],
    dirs: &StoreDirs,
    experimental: bool,
) -> Result<(), String> {
    if paths.iter().any(legacy_reserved) {
        return Err(rusty_l10n::tr("job-retained-protected"));
    }
    for path in [&dirs.config, &dirs.data] {
        let location = crate::startup::path_location(path).ok_or("invalid profile path")?;
        let resolved = match std::fs::canonicalize(path) {
            Ok(resolved) => {
                Some(crate::startup::path_location(&resolved).ok_or("invalid profile path")?)
            }
            Err(_) if !experimental => canonical_ancestor(path)
                .and_then(|resolved| crate::startup::path_location(&resolved)),
            Err(error) => return Err(error.to_string()),
        };
        if paths.iter().any(|path| {
            legacy_intersects(path, &location)
                || resolved
                    .as_ref()
                    .is_some_and(|resolved| legacy_intersects(path, resolved))
        }) {
            return Err(rusty_l10n::tr("job-profile-protected"));
        }
    }
    if !experimental {
        return Ok(());
    }
    let (db, _) = crate::crash::open_db(&dirs.journal_file()).map_err(|error| error.to_string())?;
    let catalog = RetentionCatalog::new(db).map_err(|error| error.to_string())?;
    if !catalog.has_live().map_err(|error| error.to_string())? {
        return Ok(());
    }
    if catalog
        .intersects_live(paths)
        .map_err(|error| error.to_string())?
    {
        return Err(rusty_l10n::tr("job-retained-parent-protected"));
    }
    Ok(())
}
fn legacy_reserved(path: &Location) -> bool {
    std::iter::successors(Some(path.clone()), Location::parent)
        .filter_map(|path| path.name())
        .any(|name| rusty_core::brand::is_retained_container(&name.to_text()))
}

fn legacy_intersects(a: &Location, b: &Location) -> bool {
    std::iter::successors(Some(a.clone()), Location::parent).any(|path| path.same_place(b))
        || std::iter::successors(Some(b.clone()), Location::parent).any(|path| path.same_place(a))
}

fn legacy_location(path: &Location) -> Result<Location, rusty_core::FsError> {
    let reserved = |path: &Location| {
        std::iter::successors(Some(path.clone()), Location::parent)
            .filter_map(|path| path.name())
            .any(|name| rusty_core::brand::is_reserved_name(&name.to_text()))
    };
    let refuse = |path: &Location| rusty_core::FsError {
        location: Some(path.clone()),
        ..rusty_core::FsError::new(rusty_core::FsErrorKind::AccessDenied)
    };
    if reserved(path) {
        return Err(refuse(path));
    }
    let resolved = rusty_fs::mutation_location(path)?;
    if reserved(&resolved) {
        return Err(refuse(&resolved));
    }
    Ok(resolved)
}

fn mutated(step: &Step) -> Vec<&Location> {
    match step {
        Step::Rename { source, target } => vec![source, target],
        Step::CopyFile { target, .. } | Step::MakeFolder { target } => vec![target],
        Step::MakeLink { at, .. } => vec![at],
        Step::Recycle { location }
        | Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => vec![location],
    }
}
