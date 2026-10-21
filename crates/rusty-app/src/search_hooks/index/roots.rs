use rusty_core::Location;
use rusty_index::pipe::Status;
use rusty_shell::{Drive, DriveKind};
use rusty_ui::command_center::IndexOptions;
use rusty_ui::command_center::index_source::drive_root;

pub(super) fn roots(
    options: &IndexOptions,
    available: &[Drive],
    explicit: Vec<String>,
    service: Option<&Status>,
) -> Vec<Location> {
    let requested = if !explicit.is_empty() {
        explicit
    } else if !options.drives.is_empty() {
        options.drives.iter().map(|root| drive_root(root)).collect()
    } else if service.is_some() {
        available
            .iter()
            .map(|drive| drive.root.clone())
            .chain(
                service
                    .into_iter()
                    .flat_map(|status| &status.volumes)
                    .filter(|volume| !volume.errors.is_empty())
                    .map(|volume| drive_root(&volume.root)),
            )
            .collect()
    } else {
        vec![
            crate::startup::home_location(|name| std::env::var(name).ok())
                .as_str()
                .to_owned(),
        ]
    };
    let mut roots = Vec::new();
    for root in requested {
        let Ok(root) = Location::new(root) else {
            continue;
        };
        if is_allowed(options, &root, available)
            && !covered(&root, service)
            && !roots.iter().any(|old: &Location| contains(old, &root))
        {
            roots.retain(|old| !contains(&root, old));
            roots.push(root);
        }
    }
    roots
}

pub(super) fn is_allowed(options: &IndexOptions, root: &Location, available: &[Drive]) -> bool {
    let kind = available
        .iter()
        .filter_map(|drive| {
            let location = Location::new(&drive.root).ok()?;
            contains(&location, root).then_some((drive.root.len(), drive.kind))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, kind)| kind);
    if root.as_str().starts_with(r"\\") && !options.network {
        return false;
    }
    !matches!(kind, Some(DriveKind::Network) if !options.network)
        && !matches!(kind, Some(DriveKind::Removable) if !options.removable)
}

fn contains(parent: &Location, child: &Location) -> bool {
    std::iter::successors(Some(child.clone()), Location::parent).any(|path| path.same_place(parent))
}

pub(super) fn exclusions(options: &IndexOptions, available: &[Drive]) -> Vec<String> {
    let mut exclude = options.exclude.clone();
    exclude.extend(
        available
            .iter()
            .filter(|drive| {
                matches!(drive.kind, DriveKind::Network) && !options.network
                    || matches!(drive.kind, DriveKind::Removable) && !options.removable
            })
            .map(|drive| drive.root.clone()),
    );
    exclude
}

fn covered(root: &Location, service: Option<&Status>) -> bool {
    service.is_some_and(|status| {
        status.volumes.iter().any(|volume| {
            volume.errors.is_empty()
                && (volume.entries > 0 || volume.progress.is_some())
                && Location::new(drive_root(&volume.root))
                    .is_ok_and(|volume| contains(&volume, root))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_index::pipe::VolumeStatus;

    #[test]
    fn a_volume_without_a_usable_index_is_not_covered() {
        let root = Location::new(r"C:\").unwrap();
        let building = |progress| Status {
            volumes: vec![VolumeStatus {
                root: "C:".into(),
                progress,
                ..Default::default()
            }],
        };
        assert!(covered(&root, Some(&building(Some(0)))));
        assert!(!covered(&root, Some(&building(None))));
    }

    #[test]
    fn service_coverage_leaves_network_non_ntfs_and_failed_volumes_to_the_walk() {
        let available = rusty_shell::assemble(&rusty_shell::RawDrives {
            mask: 0b11100,
            kinds: vec![
                ('C', DriveKind::Fixed),
                ('D', DriveKind::Fixed),
                ('E', DriveKind::Network),
            ],
            ..Default::default()
        });
        let status = Status {
            volumes: vec![
                VolumeStatus {
                    root: "C:".into(),
                    entries: 10,
                    ..Default::default()
                },
                VolumeStatus {
                    root: "F:".into(),
                    errors: vec!["refused".into()],
                    ..Default::default()
                },
            ],
        };
        let mut options = IndexOptions::default();
        let found = roots(&options, &available, Vec::new(), Some(&status));
        assert_eq!(
            found,
            [
                Location::new(r"D:\").unwrap(),
                Location::new(r"F:\").unwrap()
            ]
        );
        options.network = true;
        let found = roots(&options, &available, Vec::new(), Some(&status));
        assert_eq!(found.len(), 3);
        assert!(
            found
                .iter()
                .any(|root| root.same_place(&Location::new(r"E:\").unwrap()))
        );
        options.drives = vec!["D:".into()];
        assert_eq!(
            roots(&options, &available, Vec::new(), Some(&status)),
            [Location::new(r"D:\").unwrap()]
        );
    }
}
