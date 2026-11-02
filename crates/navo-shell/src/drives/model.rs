#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    Ram,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub total: u64,
    pub free: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    pub root: String,
    pub letter: Option<char>,
    pub kind: DriveKind,
    pub label: Option<String>,
    pub remote: Option<String>,
    pub connected: bool,
    pub space: Option<Space>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawDrives {
    pub mask: u32,
    pub kinds: Vec<(char, DriveKind)>,
    pub mapped: Vec<(char, String)>,
    pub remembered: Vec<(char, String)>,
    pub unavailable: Vec<char>,
}

const PSEUDO: [&str; 9] = [
    "proc",
    "sysfs",
    "tmpfs",
    "devtmpfs",
    "cgroup2",
    "overlay",
    "squashfs",
    "autofs",
    "fuse.portal",
];
const NETWORK: [&str; 6] = ["nfs", "nfs4", "cifs", "smb3", "sshfs", "fuse.sshfs"];

fn first_by_letter<T: Clone>(entries: &[(char, T)], letter: char) -> Option<T> {
    entries
        .iter()
        .find(|(found, _)| found.eq_ignore_ascii_case(&letter))
        .map(|(_, value)| value.clone())
}

fn drive(letter: char, kind: DriveKind, remote: Option<String>, connected: bool) -> Drive {
    Drive {
        root: format!("{letter}:\\"),
        letter: Some(letter),
        kind,
        label: None,
        remote,
        connected,
        space: None,
    }
}

#[must_use]
pub fn assemble(raw: &RawDrives) -> Vec<Drive> {
    (0u8..26)
        .filter_map(|bit| {
            let letter = char::from(b'A' + bit);
            let remembered = first_by_letter(&raw.remembered, letter);
            if raw.mask & (1 << bit) != 0 {
                let kind = first_by_letter(&raw.kinds, letter).unwrap_or(DriveKind::Unknown);
                let remembered = remembered.filter(|_| kind == DriveKind::Network);
                let remote = first_by_letter(&raw.mapped, letter).or(remembered);
                let connected = !raw
                    .unavailable
                    .iter()
                    .any(|found| found.eq_ignore_ascii_case(&letter));
                Some(drive(letter, kind, remote, connected))
            } else {
                remembered.map(|remote| drive(letter, DriveKind::Network, Some(remote), false))
            }
        })
        .collect()
}

fn unescape(text: &str) -> String {
    text.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn is_shown(mount: &str, user: &str) -> bool {
    let prefixes = [
        "/mnt/".to_owned(),
        "/media/".to_owned(),
        format!("/media/{user}/"),
        format!("/run/media/{user}/"),
    ];
    prefixes.iter().any(|prefix| {
        mount
            .strip_prefix(prefix.as_str())
            .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
    })
}

fn mounted(line: &str, user: &str) -> Option<Drive> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let [device, mount, fstype, _, ..] = fields.as_slice() else {
        return None;
    };
    let mount = unescape(mount);
    if PSEUDO.contains(fstype) || !is_shown(&mount, user) {
        return None;
    }
    let network = NETWORK.contains(fstype);
    let kind = if network {
        DriveKind::Network
    } else if mount.starts_with("/media/") || mount.starts_with("/run/media/") {
        DriveKind::Removable
    } else {
        DriveKind::Fixed
    };
    let label = mount.rsplit('/').next().map(str::to_owned);
    Some(Drive {
        root: mount,
        letter: None,
        kind,
        label,
        remote: network.then(|| (*device).to_owned()),
        connected: true,
        space: None,
    })
}

#[must_use]
pub fn parse_mounts(text: &str, user: &str) -> Vec<Drive> {
    let mut drives = vec![Drive {
        root: "/".to_owned(),
        letter: None,
        kind: DriveKind::Fixed,
        label: None,
        remote: None,
        connected: true,
        space: None,
    }];
    for found in text.lines().filter_map(|line| mounted(line, user)) {
        if !drives.iter().any(|drive| drive.root == found.root) {
            drives.push(found);
        }
    }
    drives
}
