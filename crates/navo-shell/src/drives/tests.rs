use super::{Drive, DriveKind, RawDrives, assemble, parse_mounts};

fn bit(letter: char) -> u32 {
    1 << (letter as u32 - 'A' as u32)
}

fn mask(letters: &str) -> u32 {
    letters.chars().map(bit).fold(0, |acc, b| acc | b)
}

fn win(letter: char, kind: DriveKind, remote: Option<&str>, connected: bool) -> Drive {
    Drive {
        root: format!("{letter}:\\"),
        letter: Some(letter),
        kind,
        label: None,
        remote: remote.map(str::to_owned),
        connected,
        space: None,
    }
}

fn fixed(letter: char) -> Drive {
    win(letter, DriveKind::Fixed, None, true)
}

fn net(letter: char, remote: &str) -> Drive {
    win(letter, DriveKind::Network, Some(remote), true)
}

fn gone(letter: char, remote: &str) -> Drive {
    win(letter, DriveKind::Network, Some(remote), false)
}

fn kinds(list: &[(char, DriveKind)]) -> Vec<(char, DriveKind)> {
    list.to_vec()
}

fn unc(list: &[(char, &str)]) -> Vec<(char, String)> {
    list.iter().map(|&(c, s)| (c, s.to_owned())).collect()
}

fn letters(drives: &[Drive]) -> Vec<Option<char>> {
    drives.iter().map(|d| d.letter).collect()
}

#[test]
fn assemble_empty_input_gives_empty_list() {
    assert_eq!(assemble(&RawDrives::default()), Vec::new());
}

#[test]
fn assemble_maps_bit_zero_to_a_and_bit_25_to_z() {
    let raw = RawDrives {
        mask: 1 | (1 << 25),
        ..RawDrives::default()
    };
    assert_eq!(letters(&assemble(&raw)), vec![Some('A'), Some('Z')]);
}

#[test]
fn assemble_sorts_by_letter_regardless_of_kinds_order() {
    let raw = RawDrives {
        mask: mask("CDE"),
        kinds: kinds(&[
            ('E', DriveKind::Optical),
            ('C', DriveKind::Fixed),
            ('D', DriveKind::Removable),
        ]),
        ..RawDrives::default()
    };
    assert_eq!(
        assemble(&raw),
        vec![
            fixed('C'),
            win('D', DriveKind::Removable, None, true),
            win('E', DriveKind::Optical, None, true),
        ]
    );
}

#[test]
fn assemble_present_letter_without_kind_is_unknown() {
    let raw = RawDrives {
        mask: mask("CR"),
        kinds: kinds(&[('C', DriveKind::Fixed)]),
        ..RawDrives::default()
    };
    assert_eq!(
        assemble(&raw),
        vec![fixed('C'), win('R', DriveKind::Unknown, None, true)]
    );
}

#[test]
fn assemble_ignores_kinds_and_mapped_for_absent_letters() {
    let raw = RawDrives {
        mask: bit('C'),
        kinds: kinds(&[('C', DriveKind::Fixed), ('D', DriveKind::Ram)]),
        mapped: unc(&[('X', r"\\nas\x")]),
        ..RawDrives::default()
    };
    assert_eq!(assemble(&raw), vec![fixed('C')]);
}

#[test]
fn assemble_mapped_drive_gets_unc_remote() {
    let raw = RawDrives {
        mask: mask("CN"),
        kinds: kinds(&[('C', DriveKind::Fixed), ('N', DriveKind::Network)]),
        mapped: unc(&[('N', r"\\nas\media")]),
        ..RawDrives::default()
    };
    assert_eq!(assemble(&raw), vec![fixed('C'), net('N', r"\\nas\media")]);
}

#[test]
fn assemble_mapped_remote_applies_to_any_kind() {
    let raw = RawDrives {
        mask: bit('S'),
        kinds: kinds(&[('S', DriveKind::Fixed)]),
        mapped: unc(&[('S', r"\\localhost\c$")]),
        ..RawDrives::default()
    };
    let expected = win('S', DriveKind::Fixed, Some(r"\\localhost\c$"), true);
    assert_eq!(assemble(&raw), vec![expected]);
}

#[test]
fn assemble_absent_remembered_drive_is_disconnected_network() {
    let raw = RawDrives {
        mask: bit('C'),
        kinds: kinds(&[('C', DriveKind::Fixed)]),
        remembered: unc(&[('M', r"\\nas\music")]),
        ..RawDrives::default()
    };
    assert_eq!(assemble(&raw), vec![fixed('C'), gone('M', r"\\nas\music")]);
}

#[test]
fn assemble_remembered_drives_interleave_in_letter_order() {
    let raw = RawDrives {
        mask: mask("CQ"),
        kinds: kinds(&[('C', DriveKind::Fixed), ('Q', DriveKind::Fixed)]),
        remembered: unc(&[('Z', r"\\nas\z"), ('B', r"\\nas\b"), ('M', r"\\nas\m")]),
        ..RawDrives::default()
    };
    let expected = vec![Some('B'), Some('C'), Some('M'), Some('Q'), Some('Z')];
    assert_eq!(letters(&assemble(&raw)), expected);
}

#[test]
fn assemble_present_drive_wins_over_remembered_and_keeps_mapped_remote() {
    let raw = RawDrives {
        mask: bit('N'),
        kinds: kinds(&[('N', DriveKind::Network)]),
        mapped: unc(&[('N', r"\\nas\live")]),
        remembered: unc(&[('N', r"\\nas\old")]),
        unavailable: Vec::new(),
    };
    let expected = net('N', r"\\nas\live");
    assert_eq!(assemble(&raw), vec![expected]);
}

#[test]
fn assemble_present_drive_without_mapping_takes_remembered_remote() {
    let raw = RawDrives {
        mask: bit('N'),
        kinds: kinds(&[('N', DriveKind::Network)]),
        remembered: unc(&[('N', r"\\nas\share")]),
        ..RawDrives::default()
    };
    let expected = net('N', r"\\nas\share");
    assert_eq!(assemble(&raw), vec![expected]);
}

#[test]
fn assemble_matches_lowercase_letters_and_outputs_uppercase() {
    let raw = RawDrives {
        mask: mask("CN"),
        kinds: kinds(&[('c', DriveKind::Fixed), ('n', DriveKind::Network)]),
        mapped: unc(&[('n', r"\\nas\n")]),
        remembered: unc(&[('m', r"\\nas\m")]),
        unavailable: Vec::new(),
    };
    assert_eq!(
        assemble(&raw),
        vec![fixed('C'), gone('M', r"\\nas\m"), net('N', r"\\nas\n")]
    );
}

#[test]
fn assemble_duplicate_entries_first_wins() {
    let raw = RawDrives {
        mask: bit('D'),
        kinds: kinds(&[('D', DriveKind::Removable), ('d', DriveKind::Fixed)]),
        mapped: unc(&[('D', r"\\first\d"), ('D', r"\\second\d")]),
        remembered: unc(&[('m', r"\\first\m"), ('M', r"\\second\m")]),
        unavailable: Vec::new(),
    };
    assert_eq!(
        assemble(&raw),
        vec![
            win('D', DriveKind::Removable, Some(r"\\first\d"), true),
            gone('M', r"\\first\m"),
        ]
    );
}

fn root_drive() -> Drive {
    Drive {
        root: "/".to_owned(),
        letter: None,
        kind: DriveKind::Fixed,
        label: None,
        remote: None,
        connected: true,
        space: None,
    }
}

fn mount(root: &str, label: &str, kind: DriveKind, remote: Option<&str>) -> Drive {
    Drive {
        root: root.to_owned(),
        letter: None,
        kind,
        label: Some(label.to_owned()),
        remote: remote.map(str::to_owned),
        connected: true,
        space: None,
    }
}

fn disk(root: &str, label: &str) -> Drive {
    mount(root, label, DriveKind::Fixed, None)
}

fn roots(drives: &[Drive]) -> Vec<&str> {
    drives.iter().map(|d| d.root.as_str()).collect()
}

#[test]
fn parse_mounts_empty_text_gives_root_only() {
    assert_eq!(parse_mounts("", "admin"), vec![root_drive()]);
}

#[test]
fn parse_mounts_root_is_first_and_not_duplicated() {
    let text = "/dev/sdb1 /mnt/data ext4 rw 0 0\n/dev/nvme0n1p2 / btrfs rw 0 0\n";
    assert_eq!(
        parse_mounts(text, "admin"),
        vec![root_drive(), disk("/mnt/data", "data"),]
    );
}

#[test]
fn parse_mounts_keeps_first_appearance_order_once_each() {
    let text = "\
/dev/sdc1 /mnt/zeta ext4 rw 0 0
/dev/sdb1 /mnt/alpha xfs rw 0 0
/dev/sdd1 /mnt/mid btrfs rw 0 0
/dev/sde1 /mnt/zeta ext4 rw 0 0
";
    let expected = vec!["/", "/mnt/zeta", "/mnt/alpha", "/mnt/mid"];
    assert_eq!(roots(&parse_mounts(text, "admin")), expected);
}

#[test]
fn parse_mounts_skips_unrelated_deeper_and_bare_mount_points() {
    let text = "\
/dev/sda1 /boot vfat rw 0 0
/dev/sda2 /home ext4 rw 0 0
/dev/sdb1 /mnt ext4 rw 0 0
/dev/sdc1 /mnt/a/b ext4 rw 0 0
/dev/sdd1 /media ext4 rw 0 0
/dev/sde1 /mntx/data ext4 rw 0 0
/dev/sdf1 /run/media/admin/stick/inner vfat rw 0 0
";
    assert_eq!(parse_mounts(text, "admin"), vec![root_drive()]);
}

#[test]
fn parse_mounts_media_and_run_media_of_user_are_removable() {
    let text = "\
/dev/sdb1 /media/usb vfat rw 0 0
/dev/sdc1 /media/admin/STICK exfat rw 0 0
/dev/sdd1 /run/media/admin/My\\040Backup ntfs3 rw 0 0
";
    assert_eq!(
        parse_mounts(text, "admin"),
        vec![
            root_drive(),
            mount("/media/usb", "usb", DriveKind::Removable, None),
            mount("/media/admin/STICK", "STICK", DriveKind::Removable, None),
            mount(
                "/run/media/admin/My Backup",
                "My Backup",
                DriveKind::Removable,
                None
            ),
        ]
    );
}

#[test]
fn parse_mounts_skips_other_users_media() {
    let text = "\
/dev/sdb1 /run/media/guest/Stick vfat rw 0 0
/dev/sdc1 /media/guest/Stick vfat rw 0 0
";
    assert_eq!(parse_mounts(text, "admin"), vec![root_drive()]);
}

#[test]
fn parse_mounts_skips_pseudo_file_systems() {
    let text = "\
proc /mnt/proc proc rw 0 0
sysfs /mnt/sys sysfs rw 0 0
tmpfs /mnt/ram tmpfs rw 0 0
devtmpfs /mnt/dev devtmpfs rw 0 0
cgroup2 /mnt/cg cgroup2 rw 0 0
overlay /mnt/ov overlay rw 0 0
/dev/loop0 /mnt/snap squashfs ro 0 0
systemd-1 /mnt/auto autofs rw 0 0
portal /media/portal fuse.portal rw 0 0
";
    assert_eq!(parse_mounts(text, "admin"), vec![root_drive()]);
}

#[test]
fn parse_mounts_network_file_systems_get_remote() {
    let text = "\
nas:/export /mnt/nfs nfs rw 0 0
nas:/export4 /mnt/nfs4 nfs4 rw 0 0
//nas/share /mnt/cifs cifs rw 0 0
//nas/smb /mnt/smb smb3 rw 0 0
admin@nas:/srv /mnt/ssh sshfs rw 0 0
admin@nas:/home /mnt/fssh fuse.sshfs rw 0 0
";
    let net =
        |root: &str, label: &str, dev: &str| mount(root, label, DriveKind::Network, Some(dev));
    assert_eq!(
        parse_mounts(text, "admin"),
        vec![
            root_drive(),
            net("/mnt/nfs", "nfs", "nas:/export"),
            net("/mnt/nfs4", "nfs4", "nas:/export4"),
            net("/mnt/cifs", "cifs", "//nas/share"),
            net("/mnt/smb", "smb", "//nas/smb"),
            net("/mnt/ssh", "ssh", "admin@nas:/srv"),
            net("/mnt/fssh", "fssh", "admin@nas:/home"),
        ]
    );
}

#[test]
fn parse_mounts_network_under_media_stays_network() {
    let text = "//nas/share /media/share cifs rw 0 0\n";
    let expected = mount(
        "/media/share",
        "share",
        DriveKind::Network,
        Some("//nas/share"),
    );
    assert_eq!(parse_mounts(text, "admin"), vec![root_drive(), expected]);
}

#[test]
fn parse_mounts_decodes_octal_escapes() {
    let text = "\
/dev/sdb1 /mnt/My\\040Disk ext4 rw 0 0
/dev/sdc1 /mnt/tab\\011here ext4 rw 0 0
/dev/sdd1 /mnt/line\\012feed ext4 rw 0 0
/dev/sde1 /mnt/back\\134slash ext4 rw 0 0
";
    assert_eq!(
        parse_mounts(text, "admin"),
        vec![
            root_drive(),
            disk("/mnt/My Disk", "My Disk"),
            disk("/mnt/tab\there", "tab\there"),
            disk("/mnt/line\nfeed", "line\nfeed"),
            disk("/mnt/back\\slash", "back\\slash"),
        ]
    );
}

#[test]
fn parse_mounts_skips_malformed_lines() {
    let text = "\
garbage
/dev/sdb1 /mnt/half

   
/dev/sdc1 /mnt/good ext4 rw 0 0
";
    assert_eq!(
        parse_mounts(text, "admin"),
        vec![root_drive(), disk("/mnt/good", "good"),]
    );
}
