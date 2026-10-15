use std::sync::PoisonError;

use super::{DriveKind, LengthClass, PathKind, RedactedPath, Redactor};

#[derive(PartialEq, Eq)]
enum Root<'a> {
    Drive(char),
    Unc(Option<&'a str>, Option<&'a str>),
    Unix,
    Relative,
}

struct ParsedPath<'a> {
    root: Root<'a>,
    segments: Vec<&'a str>,
    long: bool,
}

impl<'a> ParsedPath<'a> {
    fn parse(path: &'a str) -> Self {
        let (path, long) = path
            .strip_prefix(r"\\?\")
            .map_or((path, false), |path| (path, true));
        let unc = long
            .then(|| path.split_once(['\\', '/']))
            .flatten()
            .filter(|(prefix, _)| prefix.eq_ignore_ascii_case("UNC"));
        let (root, rest) = if let Some((_, rest)) = unc {
            (Root::Unc(None, None), rest)
        } else {
            parse_root(path)
        };
        let mut segments = rest.split(['\\', '/']).filter(|part| !part.is_empty());
        let root = match root {
            Root::Unc(_, _) => Root::Unc(segments.next(), segments.next()),
            root => root,
        };
        Self {
            root,
            segments: segments.collect(),
            long,
        }
    }

    fn profile_depth(&self, profile: &str) -> Option<usize> {
        let profile = ParsedPath::parse(profile);
        let windows = matches!(self.root, Root::Drive(_));
        if self.root != profile.root || !matches!(self.root, Root::Drive(_) | Root::Unix) {
            return None;
        }
        let count = profile.segments.len();
        let matches = self.segments.iter().zip(&profile.segments).all(|(a, b)| {
            if windows {
                a.to_lowercase() == b.to_lowercase()
            } else {
                a == b
            }
        });
        (self.segments.len() >= count && matches).then_some(count)
    }
}

fn parse_root(path: &str) -> (Root<'_>, &str) {
    let mut chars = path.chars();
    if let (Some(letter), Some(':')) = (chars.next(), chars.next())
        && letter.is_ascii_alphabetic()
    {
        return (Root::Drive(letter.to_ascii_uppercase()), chars.as_str());
    }
    let root = if path.starts_with(r"\\") {
        Root::Unc(None, None)
    } else if path.starts_with('/') {
        Root::Unix
    } else {
        Root::Relative
    };
    (root, path)
}

impl Redactor {
    #[must_use]
    pub fn path(&self, path: &str, kind: PathKind, drive: DriveKind) -> RedactedPath {
        let parsed = ParsedPath::parse(path);
        let length = path.chars().count();
        RedactedPath {
            text: self.render_path(&parsed, kind),
            drive,
            depth: parsed.segments.len(),
            length_class: length_class(length),
            long: parsed.long || length >= 260,
            non_ascii: !path.is_ascii(),
        }
    }

    fn render_path(&self, path: &ParsedPath<'_>, kind: PathKind) -> String {
        let separator = match path.root {
            Root::Drive(_) | Root::Unc(_, _) => '\\',
            _ => '/',
        };
        let mut output = self.render_root(&path.root);
        let profile = self
            .identity
            .profile
            .as_deref()
            .filter(|profile| !profile.is_empty())
            .and_then(|profile| path.profile_depth(profile));
        if profile.is_some() {
            output.push_str("<user>");
        }
        for (index, segment) in path.segments.iter().enumerate().skip(profile.unwrap_or(0)) {
            if !output.is_empty() && !output.ends_with(separator) {
                output.push(separator);
            }
            let file = kind == PathKind::File && index + 1 == path.segments.len();
            output.push_str(&self.render_segment(segment, file));
        }
        output
    }

    fn render_root(&self, root: &Root<'_>) -> String {
        match root {
            Root::Drive(letter) => format!(r"{letter}:\"),
            Root::Unix => "/".to_owned(),
            Root::Relative => String::new(),
            Root::Unc(server, share) => self
                .servers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .root(*server, *share),
        }
    }

    fn render_segment(&self, segment: &str, file: bool) -> String {
        let tag = if file { 'f' } else { 'd' };
        let mut output = format!("<{tag}:{}>", self.salt.token(segment));
        if file && let Some(extension) = extension(segment) {
            output.push('.');
            output.push_str(&extension.to_ascii_lowercase());
        }
        output
    }
}

pub(super) fn extension(segment: &str) -> Option<&str> {
    let (_, extension) = segment.rsplit_once('.')?;
    ((1..=8).contains(&extension.len()) && extension.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .then_some(extension)
}

fn length_class(length: usize) -> LengthClass {
    match length {
        0..64 => LengthClass::Short,
        64..260 => LengthClass::Medium,
        _ => LengthClass::Long,
    }
}
