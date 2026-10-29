use rusty_core::{Location, ProviderPath};

const WEB_SCHEMES: [&str; 3] = ["http", "https", "mailto"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Link {
    Web,
    Anchor(String),
    Local(Vec<String>),
    Refused,
}

fn scheme(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once(':')?;
    let mut chars = scheme.chars();
    let is_scheme = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'));
    is_scheme.then_some(scheme)
}

fn segments(path: &str) -> Option<Vec<String>> {
    let mut kept: Vec<String> = Vec::new();
    for segment in path.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => {
                kept.pop()?;
            }
            name => kept.push(name.to_owned()),
        }
    }
    (!kept.is_empty()).then_some(kept)
}

pub(super) fn slug(heading: &str) -> String {
    heading
        .trim()
        .chars()
        .filter(|ch| ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .map(|ch| if ch == ' ' { '-' } else { ch })
        .collect()
}

pub(super) fn classify(url: &str) -> Link {
    let url = url.trim();
    if let Some(anchor) = url.strip_prefix('#').filter(|anchor| !anchor.is_empty()) {
        return Link::Anchor(anchor.to_lowercase());
    }
    if let Some(scheme) = scheme(url) {
        let is_web = WEB_SCHEMES
            .iter()
            .any(|web| web.eq_ignore_ascii_case(scheme));
        return if is_web { Link::Web } else { Link::Refused };
    }
    let path = url.split(['#', '?']).next().unwrap_or_default();
    if path.starts_with(['/', '\\']) {
        return Link::Refused;
    }
    segments(path).map_or(Link::Refused, Link::Local)
}

pub(super) fn resolve(file: &Location, segments: &[String]) -> Option<Location> {
    segments.iter().try_fold(file.parent()?, |folder, name| {
        folder.join(&ProviderPath::from_text(name)).ok()
    })
}

#[cfg(test)]
mod tests {
    use rusty_core::Location;

    use super::{Link, classify, resolve, slug};

    fn local(parts: &[&str]) -> Link {
        Link::Local(parts.iter().map(|part| (*part).to_owned()).collect())
    }

    #[test]
    fn only_http_https_and_mailto_open_outside_ferrite() {
        for url in [
            "https://example.com",
            "HTTP://x.org/a",
            "mailto:admin@x.org",
        ] {
            assert_eq!(classify(url), Link::Web, "{url}");
        }
        let refused = [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "ms-settings:privacy",
            "data:text/html,hi",
            "C:\\Windows\\notepad.exe",
            "/etc/passwd",
            "\\\\server\\share\\x.md",
            "../secret.md",
            "docs/../../up.md",
            "",
        ];
        for url in refused {
            assert_eq!(classify(url), Link::Refused, "{url}");
        }
    }

    #[test]
    fn a_fragment_link_names_a_heading_on_the_same_page() {
        assert_eq!(
            classify("#Start-Here"),
            Link::Anchor("start-here".to_owned())
        );
        assert_eq!(
            slug("Layouts, panes and workspaces"),
            "layouts-panes-and-workspaces"
        );
    }

    #[test]
    fn relative_links_stay_inside_the_folder_tree() {
        assert_eq!(classify("docs/setup.md"), local(&["docs", "setup.md"]));
        assert_eq!(classify("./a.md#usage"), local(&["a.md"]));
        assert_eq!(classify("docs/../b.md?x"), local(&["b.md"]));
        let file = Location::new("/home/admin/notes/readme.md").unwrap();
        let target = resolve(&file, &["docs".to_owned(), "setup.md".to_owned()]).unwrap();
        assert_eq!(target.as_str(), "/home/admin/notes/docs/setup.md");
    }
}
