use super::model::Space;

pub fn mounts_text() -> String {
    std::fs::read_to_string("/proc/self/mounts").unwrap_or_default()
}

pub fn details(root: &str) -> (Option<String>, Option<Space>) {
    let space = rustix::fs::statvfs(root).ok().map(|stat| Space {
        total: stat.f_blocks.saturating_mul(stat.f_frsize),
        free: stat.f_bavail.saturating_mul(stat.f_frsize),
    });
    (None, space)
}
