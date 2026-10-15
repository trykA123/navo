use proptest::prelude::*;

use super::{DriveKind, Identity, PathKind, Redactor, Salt, segment_tokens, strip_tokens};

const EXTENSIONS: [&str; 4] = ["jpg", "txt", "gz", "md"];

#[derive(Debug, Clone, Copy)]
enum Root {
    Drive,
    Profile,
    Unc,
    Unix,
    UnixProfile,
    Relative,
}

impl Root {
    fn prefix(self) -> &'static str {
        match self {
            Self::Drive => r"C:\",
            Self::Profile => r"C:\Users\admin\",
            Self::Unc => r"\\srv\share\",
            Self::Unix => "/",
            Self::UnixProfile => "/home/admin/",
            Self::Relative => "",
        }
    }

    fn separator(self) -> char {
        match self {
            Self::Unix | Self::UnixProfile => '/',
            _ => '\\',
        }
    }

    fn redactor(self, salt: [u8; 16]) -> Redactor {
        let profile = match self {
            Self::Unix | Self::UnixProfile => "/home/admin",
            _ => r"C:\Users\admin",
        };
        let identity = Identity {
            user: Some("admin".to_owned()),
            profile: Some(profile.to_owned()),
            ..Identity::default()
        };
        Redactor::new(Salt::from_bytes(salt), identity)
    }
}

#[derive(Debug, Clone)]
struct Case {
    root: Root,
    segments: Vec<String>,
    extension: Option<&'static str>,
    salt: [u8; 16],
}

impl Case {
    fn path(&self) -> String {
        let sep = self.root.separator().to_string();
        let mut path = format!("{}{}", self.root.prefix(), self.segments.join(&sep));
        if let Some(ext) = self.extension {
            path.push('.');
            path.push_str(ext);
        }
        path
    }

    fn kind(&self) -> PathKind {
        if self.extension.is_some() {
            PathKind::File
        } else {
            PathKind::Folder
        }
    }

    fn unredacted(&self, output: &str) -> String {
        let mut rest = strip_tokens(output).to_lowercase();
        if let Some(ext) = self.extension {
            rest = rest.replace(&format!(".{ext}"), "\u{1}");
        }
        rest
    }
}

fn segment(spaces: bool) -> impl Strategy<Value = String> {
    let pattern = if spaces {
        "[a-zA-Z0-9 \u{e9}\u{fc}\u{f8}\u{e7}\u{f1}\u{436}\u{65e5}\u{672c}]{3,12}"
    } else {
        "[a-zA-Z0-9\u{e9}\u{fc}\u{f8}\u{e7}\u{f1}\u{436}\u{65e5}\u{672c}]{3,12}"
    };
    proptest::string::string_regex(pattern).unwrap_or_else(|e| unreachable!("{e}"))
}

fn dotted_segment() -> impl Strategy<Value = String> {
    let word = "([a-zA-Z0-9.\u{e9}\u{436}]{1,6}|[0-9]{1,4}\\.[0-9]{1,2})";
    let pattern = format!("{word}( {word}){{0,2}}");
    proptest::string::string_regex(&pattern).unwrap_or_else(|e| unreachable!("{e}"))
}

fn dotted_case(roots: &'static [Root], extension: bool) -> impl Strategy<Value = Case> {
    let extension = if extension {
        proptest::sample::select(&EXTENSIONS[..])
            .prop_map(Some)
            .boxed()
    } else {
        prop::option::of(proptest::sample::select(&EXTENSIONS[..])).boxed()
    };
    (
        proptest::sample::select(roots),
        prop::collection::vec(dotted_segment(), 1..=6),
        extension,
        any::<[u8; 16]>(),
    )
        .prop_map(|(root, segments, extension, salt)| Case {
            root,
            segments,
            extension,
            salt,
        })
}

fn case(roots: &'static [Root], spaces: bool) -> impl Strategy<Value = Case> {
    (
        proptest::sample::select(roots),
        prop::collection::vec(segment(spaces), 1..=6),
        prop::option::of(proptest::sample::select(&EXTENSIONS[..])),
        any::<[u8; 16]>(),
    )
        .prop_map(|(root, segments, extension, salt)| Case {
            root,
            segments,
            extension,
            salt,
        })
}

const PATH_ROOTS: &[Root] = &[
    Root::Drive,
    Root::Profile,
    Root::Unc,
    Root::Unix,
    Root::UnixProfile,
    Root::Relative,
];
const TEXT_ROOTS: &[Root] = &[Root::Drive, Root::Profile, Root::Unc, Root::UnixProfile];
const SHAPE_ROOTS: &[Root] = &[Root::Drive, Root::Unc, Root::Unix, Root::Relative];

fn leaked(case: &Case, output: &str) -> Option<String> {
    let rest = case.unredacted(output);
    case.segments.iter().find_map(|segment| {
        let chars: Vec<char> = segment.chars().collect();
        chars
            .windows(3)
            .map(|w| w.iter().collect::<String>().to_lowercase())
            .filter(|w| !w.trim().is_empty())
            .find(|w| rest.contains(w.as_str()))
    })
}

fn path_like() -> impl Strategy<Value = String> {
    "[\\\\/:?.a-zA-Z0-9 \u{e9}<>|'\"]{0,40}"
}

proptest! {
    #[test]
    fn path_output_holds_no_part_of_any_segment(case in case(PATH_ROOTS, true)) {
        let out = case.root.redactor(case.salt).path(&case.path(), case.kind(), DriveKind::Local);
        let leak = leaked(&case, out.as_str());
        prop_assert!(leak.is_none(), "{:?} leaked into {:?}", leak, out.as_str());
    }

    #[test]
    fn text_output_holds_no_part_of_any_segment(case in case(TEXT_ROOTS, true)) {
        let line = format!("-> {} <-", case.path());
        let out = case.root.redactor(case.salt).text(&line);
        let leak = leaked(&case, &out);
        prop_assert!(leak.is_none(), "{:?} leaked into {:?}", leak, out);
    }

    #[test]
    fn path_output_with_dotted_segments_holds_no_segment_text(case in dotted_case(PATH_ROOTS, false)) {
        let out = case.root.redactor(case.salt).path(&case.path(), case.kind(), DriveKind::Local);
        let leak = leaked(&case, out.as_str());
        prop_assert!(leak.is_none(), "{:?} leaked into {:?}", leak, out.as_str());
    }

    #[test]
    fn wrapped_text_with_dotted_segments_holds_no_segment_text(case in dotted_case(TEXT_ROOTS, true)) {
        let line = format!("-> {} <-", case.path());
        let out = case.root.redactor(case.salt).text(&line);
        let leak = leaked(&case, &out);
        prop_assert!(leak.is_none(), "{:?} leaked into {:?}", leak, out);
    }

    #[test]
    fn unwrapped_text_with_trailing_words_holds_no_segment_text(case in dotted_case(TEXT_ROOTS, true)) {
        let line = format!("{} \u{3bb}\u{3ac}\u{3b8}\u{3bf}\u{3c2} \u{3be}\u{3b1}\u{3bd}\u{3ac}", case.path());
        let out = case.root.redactor(case.salt).text(&line);
        let leak = leaked(&case, &out);
        prop_assert!(leak.is_none(), "{:?} leaked into {:?}", leak, out);
    }

    #[test]
    fn token_count_equals_depth(case in case(SHAPE_ROOTS, true)) {
        let out = case.root.redactor(case.salt).path(&case.path(), case.kind(), DriveKind::Local);
        prop_assert_eq!(segment_tokens(out.as_str()).len(), out.depth(), "{}", out.as_str());
    }

    #[test]
    fn path_never_panics_on_arbitrary_strings(
        input in prop_oneof![any::<String>(), path_like()],
        file in any::<bool>(),
    ) {
        let kind = if file { PathKind::File } else { PathKind::Folder };
        let _ = Root::Drive.redactor([0; 16]).path(&input, kind, DriveKind::Unknown);
    }

    #[test]
    fn text_never_panics_on_arbitrary_strings(input in prop_oneof![any::<String>(), path_like()]) {
        let _ = Root::Drive.redactor([0; 16]).text(&input);
    }
}
