use rusty_core::Location;
use rusty_index::pipe::Status;
use rusty_shell::{Drive, DriveKind};
use rusty_ui::command_center::IndexOptions;
use rusty_ui::command_center::index_source::drive_root;

#[derive(Clone, Copy, Default)]
pub(super) struct Service<'a> {
    pub(super) reply: Option<&'a Status>,
    pub(super) volumes: Option<&'a [Location]>,
}

impl Service<'_> {
    fn indexes(&self, root: &Location) -> bool {
        self.volumes
            .is_some_and(|volumes| volumes.iter().any(|volume| contains(volume, root)))
    }
}

pub(super) fn roots(
    options: &IndexOptions,
    available: &[Drive],
    explicit: Vec<String>,
    service: Service<'_>,
) -> Vec<Location> {
    let requested = if !explicit.is_empty() {
        explicit
    } else if !options.drives.is_empty() {
        options.drives.iter().map(|root| drive_root(root)).collect()
    } else if service.reply.is_some() || service.volumes.is_some() {
        available
            .iter()
            .map(|drive| drive.root.clone())
            .chain(
                service
                    .reply
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
            && !covered(&root, service.reply)
            && !service.indexes(&root)
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

    fn replied(status: &Status) -> Service<'_> {
        Service {
            reply: Some(status),
            volumes: None,
        }
    }

    #[test]
    fn an_installed_service_that_does_not_answer_leaves_no_local_ntfs_root_to_walk() {
        let available = rusty_shell::assemble(&rusty_shell::RawDrives {
            mask: 0b11100,
            kinds: vec![
                ('C', DriveKind::Fixed),
                ('D', DriveKind::Fixed),
                ('E', DriveKind::Network),
            ],
            ..Default::default()
        });
        let ntfs = [Location::new(r"C:\").unwrap()];
        let installed = Service {
            reply: None,
            volumes: Some(&ntfs),
        };
        let options = IndexOptions {
            network: true,
            ..IndexOptions::default()
        };
        assert_eq!(
            roots(&options, &available, Vec::new(), installed),
            [
                Location::new(r"D:\").unwrap(),
                Location::new(r"E:\").unwrap()
            ]
        );
        let home = vec![r"C:\Users\admin".to_owned()];
        assert!(roots(&options, &available, home, installed).is_empty());
    }

    #[test]
    fn drives_the_service_does_not_select_are_still_walked_in_auto() {
        use crate::index_service::{ServiceDrive, selected_roots};
        let available = rusty_shell::assemble(&rusty_shell::RawDrives {
            mask: 0b11100,
            kinds: vec![
                ('C', DriveKind::Fixed),
                ('D', DriveKind::Fixed),
                ('E', DriveKind::Removable),
            ],
            ..Default::default()
        });
        let service_drives =
            [('C', true), ('D', true), ('E', false)].map(|(letter, fixed)| ServiceDrive {
                letter,
                serial: 1,
                fixed,
                ntfs: true,
            });
        let covered = |options: &IndexOptions| -> Vec<Location> {
            let search = super::super::presence::Selection::of(options).search();
            selected_roots(&search, &service_drives)
                .unwrap()
                .iter()
                .map(|root| Location::new(root).unwrap())
                .collect()
        };
        let walked = |options: &IndexOptions, explicit: Vec<String>| {
            let volumes = covered(options);
            let service = Service {
                reply: None,
                volumes: Some(&volumes),
            };
            roots(options, &available, explicit, service)
        };
        let mut options = IndexOptions {
            removable: true,
            ..IndexOptions::default()
        };
        assert_eq!(
            walked(&options, Vec::new()),
            [Location::new(r"E:\").unwrap()]
        );
        options.drives = vec!["E:".into()];
        assert_eq!(
            walked(&options, vec![r"C:\Work".into(), r"E:\Photos".into()]),
            [Location::new(r"C:\Work").unwrap()]
        );
    }

    #[test]
    fn without_the_service_the_walk_falls_back_to_the_home_folder() {
        let found = roots(
            &IndexOptions::default(),
            &[],
            Vec::new(),
            Service::default(),
        );
        assert_eq!(found.len(), 1);
    }

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
        let found = roots(&options, &available, Vec::new(), replied(&status));
        assert_eq!(
            found,
            [
                Location::new(r"D:\").unwrap(),
                Location::new(r"F:\").unwrap()
            ]
        );
        options.network = true;
        let found = roots(&options, &available, Vec::new(), replied(&status));
        assert_eq!(found.len(), 3);
        assert!(
            found
                .iter()
                .any(|root| root.same_place(&Location::new(r"E:\").unwrap()))
        );
        options.drives = vec!["D:".into()];
        assert_eq!(
            roots(&options, &available, Vec::new(), replied(&status)),
            [Location::new(r"D:\").unwrap()]
        );
    }
}
