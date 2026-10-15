use super::{Location, ProviderPath, windows_style};

impl Location {
    #[must_use]
    pub fn name(&self) -> Option<ProviderPath> {
        let parent = self.parent()?;
        let rest = self.as_str().strip_prefix(parent.as_str())?;
        let name = rest.trim_start_matches(['\\', '/']);
        (!name.is_empty()).then(|| ProviderPath::from_text(name))
    }

    #[must_use]
    pub fn root(&self) -> Self {
        if !self.is_local() {
            return Self::provider_root(self.provider.clone());
        }
        let text = self.path.to_text();
        let windows = windows_style(&self.text);
        let mut root = text[..root_len(&text, windows)].to_owned();
        if windows && !root.ends_with(['\\', '/']) {
            root.push('\\');
        }
        if root.is_empty() {
            root.push('/');
        }
        Self::with_provider(self.provider.clone(), ProviderPath::from_text(&root))
            .unwrap_or_else(|_| self.clone())
    }

    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let text = self.path.to_text();
        let windows = self.is_local() && windows_style(&self.text);
        let is_separator = |c: char| c == '/' || (windows && c == '\\');
        let root = root_len(&text, windows);
        let trimmed = text.trim_end_matches(is_separator);
        let trimmed = if trimmed.len() < root {
            &text[..root]
        } else {
            trimmed
        };
        if trimmed.len() <= root {
            return None;
        }
        let parent = match trimmed[root..].rfind(is_separator) {
            Some(index) => &trimmed[..root + index],
            None if root == 0 => return Some(Self::provider_root(self.provider.clone())),
            None => &text[..root],
        };
        let parent = if parent.len() < root {
            &text[..root]
        } else {
            parent
        };
        Self::with_provider(self.provider.clone(), ProviderPath::from_text(parent)).ok()
    }
}

pub(super) fn root_len(text: &str, windows: bool) -> usize {
    if !windows {
        return usize::from(text.starts_with('/'));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        let unc = rest
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"UNC\"));
        return if unc {
            8 + share_root_len(&rest[4..])
        } else {
            4 + drive_root_len(rest)
        };
    }
    if let Some(rest) = text.strip_prefix(r"\\") {
        return 2 + share_root_len(rest);
    }
    drive_root_len(text)
}

fn drive_root_len(text: &str) -> usize {
    match text.as_bytes() {
        [_, b':', b'\\' | b'/', ..] => 3,
        [_, b':', ..] => 2,
        _ => 0,
    }
}

fn share_root_len(rest: &str) -> usize {
    let mut separators = rest.match_indices(['\\', '/']).map(|(index, _)| index);
    match (separators.next(), separators.next()) {
        (Some(_), Some(second)) => second,
        _ => rest.len(),
    }
}
