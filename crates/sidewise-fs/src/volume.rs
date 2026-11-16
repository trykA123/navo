//! Volume ids: the unit that io-queue limits and drive-class caches are keyed by.

use navo_core::Location;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VolumeId(pub String);

impl VolumeId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn root(&self) -> Option<Location> {
        if self.0.contains("://") {
            return None;
        }
        let root = if self.0 == "/" {
            self.0.clone()
        } else {
            format!("{}\\", self.0)
        };
        Location::new(root).ok()
    }
}

#[must_use]
pub fn volume_of(location: &Location) -> VolumeId {
    if !location.is_local() {
        return VolumeId(format!("{}://", location.provider().as_str()));
    }
    VolumeId(local_volume(location.as_str()))
}

fn local_volume(text: &str) -> String {
    const VERBATIM_UNC: &str = r"\\?\UNC\";
    if let Some(rest) = text
        .get(..VERBATIM_UNC.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(VERBATIM_UNC))
        .and_then(|_| text.get(VERBATIM_UNC.len()..))
    {
        return unc_volume(rest);
    }
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    if let Some(rest) = text.strip_prefix(r"\\") {
        return unc_volume(rest);
    }
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
            format!("{}:", letter.to_ascii_uppercase())
        }
        _ => "/".to_owned(),
    }
}

fn unc_volume(rest: &str) -> String {
    let mut parts = rest.split('\\');
    let server = parts.next().unwrap_or_default().to_lowercase();
    let share = parts.next().unwrap_or_default().to_lowercase();
    format!(r"\\{server}\{share}")
}
