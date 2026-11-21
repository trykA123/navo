//! What one command line asks of the app: print a line, refuse with exit code 2, or open the
//! window with an optional first compare tab. Pure, so `main` runs it before any window exists.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use navo_compare_cli::args::{Action, SideSpec, parse};
use navo_compare_profile::brand;

/// The `--help` text.
pub const USAGE: &str = "Usage: sidewise [LEFT RIGHT] [--profile DIR]\n       sidewise --version | --help\n\nCompares two folders, two files or two Git revisions (repo@rev) side by side.\nWith no sides it opens the start screen.";

/// What `main` does with the command line.
#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// Print the text on stdout and exit 0.
    Print(String),
    /// Print the text on stderr and exit 2 (`Exit::Error`); no window opens.
    Refuse(String),
    /// Open the window.
    Open(Opening),
}

/// How the window opens: an optional first compare tab and the profile folder.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Opening {
    pub pair: Option<(SideSpec, SideSpec)>,
    pub profile: Option<PathBuf>,
}

/// Decides what `args` (without the program name) ask for; relative paths join `cwd`; `exists` tests a path.
/// `--wait`, `--new-window`, `--read-only`, `--left-title` and `--right-title` are parsed and ignored.
pub fn decide(args: &[OsString], cwd: &Path, exists: &dyn Fn(&Path) -> bool) -> Launch {
    let invocation = match parse(args, cwd, exists) {
        Ok(invocation) => invocation,
        Err(error) => return refuse(&error.to_string()),
    };
    let profile = invocation.profile.map(|dir| cwd.join(dir));
    match invocation.action {
        Action::Version => Launch::Print(format!("{} {}", brand::SLUG, env!("CARGO_PKG_VERSION"))),
        Action::Help => Launch::Print(USAGE.to_owned()),
        Action::Start { left: None } => Launch::Open(Opening {
            pair: None,
            profile,
        }),
        Action::Start { left: Some(_) } => {
            refuse("give two sides (two folders, two files or Git revisions), or none.")
        }
        Action::Compare { left, right }
            if matches!(left, SideSpec::GitUrl { .. })
                || matches!(right, SideSpec::GitUrl { .. }) =>
        {
            refuse(&sidewise_l10n::t!("git-remote-cli-refused"))
        }
        Action::Compare { left, right } => Launch::Open(Opening {
            pair: Some((left, right)),
            profile,
        }),
        Action::SelectLeft { .. } | Action::CompareWith { .. } => {
            refuse("`--select-left` and `--compare-with` are not in this alpha.")
        }
    }
}

/// `Launch::Refuse(format!("{}: {text}", brand::SLUG))`.
fn refuse(text: &str) -> Launch {
    Launch::Refuse(format!("{}: {text}", brand::SLUG))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `decide` over `args`, where only `names` exist under the temp folder.
    fn run(args: &[&str], names: &[&str]) -> Launch {
        let cwd = std::env::temp_dir();
        let args: Vec<OsString> = args.iter().map(|arg| OsString::from(*arg)).collect();
        decide(&args, &cwd, &|path| {
            names.iter().any(|name| cwd.join(name) == path)
        })
    }

    /// The temp folder the tests join names against.
    fn cwd() -> PathBuf {
        std::env::temp_dir()
    }

    /// Two names under the temp folder, as a compare pair.
    fn pair(left: &str, right: &str) -> Option<(SideSpec, SideSpec)> {
        Some((
            SideSpec::Path(cwd().join(left)),
            SideSpec::Path(cwd().join(right)),
        ))
    }

    /// Two existing paths open on the first compare tab, in order.
    #[test]
    fn two_paths_open_a_first_compare_tab() {
        let expected = Opening {
            pair: pair("a", "b"),
            profile: None,
        };
        assert_eq!(run(&["a", "b"], &["a", "b"]), Launch::Open(expected));
    }

    /// No sides leave the pair empty and the profile unset.
    #[test]
    fn no_sides_open_the_start_screen() {
        assert_eq!(run(&[], &[]), Launch::Open(Opening::default()));
    }

    /// Two Git sides, or a Git side and a path, open a first compare tab.
    #[test]
    fn a_git_pair_opens() {
        let git = SideSpec::GitRev {
            repo: cwd().join("repo"),
            rev: "main".to_owned(),
        };
        let expected = Opening {
            pair: Some((git, SideSpec::Path(cwd().join("b")))),
            profile: None,
        };
        assert_eq!(
            run(&["repo@main", "b"], &["repo", "b"]),
            Launch::Open(expected)
        );
    }

    /// One Git side alone is refused and asks for two sides or none.
    #[test]
    fn one_git_side_is_refused() {
        let refusal = Launch::Refuse(
            "sidewise: give two sides (two folders, two files or Git revisions), or none."
                .to_owned(),
        );
        assert_eq!(run(&["repo@main"], &["repo"]), refusal);
    }

    /// One existing path is refused and asks for two sides or none.
    #[test]
    fn one_path_is_refused() {
        let refusal = Launch::Refuse(
            "sidewise: give two sides (two folders, two files or Git revisions), or none."
                .to_owned(),
        );
        assert_eq!(run(&["a"], &["a"]), refusal);
    }

    /// The two pick flags are refused before any window.
    #[test]
    fn pick_flags_are_refused() {
        let refusal = Launch::Refuse(
            "sidewise: `--select-left` and `--compare-with` are not in this alpha.".to_owned(),
        );
        assert_eq!(run(&["--select-left", "a"], &["a"]), refusal);
        assert_eq!(
            run(&["--compare-with", "b"], &["b"]),
            Launch::Refuse(
                "sidewise: `--select-left` and `--compare-with` are not in this alpha.".to_owned()
            )
        );
    }

    /// A parser error keeps its sentence, with the product prefix.
    #[test]
    fn parser_errors_keep_their_text() {
        assert_eq!(
            run(&["--nope"], &[]),
            Launch::Refuse("sidewise: unknown option \"--nope\".".to_owned())
        );
        assert_eq!(
            run(&["a", "b", "c"], &[]),
            Launch::Refuse("sidewise: three-way merge arrives in a later release.".to_owned())
        );
    }

    /// The version line and the usage text are printed, not refused.
    #[test]
    fn version_and_help_print() {
        assert_eq!(
            run(&["--version"], &[]),
            Launch::Print(format!("sidewise {}", env!("CARGO_PKG_VERSION")))
        );
        assert_eq!(run(&["--help"], &[]), Launch::Print(USAGE.to_owned()));
    }

    /// The profile joins `cwd`, while the ignored flags still parse and change nothing.
    #[test]
    fn profile_joins_cwd_and_ignored_flags_pass() {
        let expected = Opening {
            pair: pair("a", "b"),
            profile: Some(cwd().join("p")),
        };
        assert_eq!(
            run(
                &[
                    "--profile",
                    "p",
                    "--wait",
                    "--read-only",
                    "--new-window",
                    "--left-title",
                    "L",
                    "a",
                    "b",
                ],
                &["a", "b"],
            ),
            Launch::Open(expected)
        );
    }

    /// A remote address on the command line is refused with the window hint (spec 7).
    #[test]
    fn remote_side_is_refused() {
        let Launch::Refuse(text) = run(&["https://github.com/o/r@main", r"C:\x"], &[]) else {
            panic!("expected a refusal");
        };
        assert!(
            text.ends_with("Remote Git sides are typed in the window, not on the command line.")
        );
    }

    /// A bad address names the typed text.
    #[test]
    fn bad_address_is_refused_by_name() {
        let Launch::Refuse(text) = run(&["http://github.com/o/r", "b"], &[]) else {
            panic!("expected a refusal");
        };
        assert!(text.contains("http://github.com/o/r"));
    }
}
