//! Builds a Windows command line from a program and its arguments, quoted so the child's C
//! runtime splits it back into the same arguments (a copy of `navo-shell`'s `helper_job/cmdline.rs`;
//! a shared crate follows when a third copy appears), and checks that a launch path is absolute.

/// Joins the program and the arguments (UTF-16 code units each) into one command line.
/// The program is always quoted (a path holds no `"`); each argument follows the C runtime rules.
pub(crate) fn command_line<'a>(
    program: &[u16],
    args: impl IntoIterator<Item = &'a [u16]>,
) -> Vec<u16> {
    let mut line = Vec::with_capacity(program.len() + 2);
    line.push(QUOTE);
    line.extend_from_slice(program);
    line.push(QUOTE);
    for arg in args {
        line.push(u16::from(b' '));
        push_arg(&mut line, arg);
    }
    line
}

const QUOTE: u16 = b'"' as u16;
const BACKSLASH: u16 = b'\\' as u16;
const SLASH: u16 = b'/' as u16;
const COLON: u16 = b':' as u16;
const QUESTION: u16 = b'?' as u16;

/// Appends one argument: bare when it has no space, tab, newline or quote; otherwise quoted, with
/// each run of backslashes doubled when a quote (or the closing quote) follows it.
fn push_arg(line: &mut Vec<u16>, arg: &[u16]) {
    let special = |unit: &u16| b" \t\n\x0b\"".iter().any(|&byte| u16::from(byte) == *unit);
    if !arg.is_empty() && !arg.iter().any(special) {
        line.extend_from_slice(arg);
        return;
    }
    line.push(QUOTE);
    let mut backslashes = 0;
    for &unit in arg {
        if unit == BACKSLASH {
            backslashes += 1;
            continue;
        }
        let run = if unit == QUOTE {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        line.extend(std::iter::repeat_n(BACKSLASH, run));
        line.push(unit);
        backslashes = 0;
    }
    line.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2));
    line.push(QUOTE);
}

/// Whether `path` (UTF-16 code units) is an absolute Windows path: a drive and a root (`C:\`),
/// a UNC server and share (`\\server\share`), or a verbatim prefix (`\\?\C:\`). A bare name,
/// `C:name` and `\name` resolve against the current folder or drive, and a host-only `\\server`
/// names no share, so they are not.
pub(crate) fn is_absolute(path: &[u16]) -> bool {
    let is_separator = |unit: &u16| *unit == BACKSLASH || *unit == SLASH;
    match path {
        [first, second, rest @ ..] if is_separator(first) && is_separator(second) => {
            let mut parts = rest.split(is_separator);
            rest.first() == Some(&QUESTION)
                || (parts.next().is_some_and(|server| !server.is_empty())
                    && parts.next().is_some_and(|share| !share.is_empty()))
        }
        [first, second, third, ..] => {
            let drive = u8::try_from(*first).is_ok_and(|byte| byte.is_ascii_alphabetic());
            drive && *second == COLON && is_separator(third)
        }
        _ => false,
    }
}
