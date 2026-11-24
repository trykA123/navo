//! Builds a Windows command line from a program and its arguments, quoted so the child's C
//! runtime splits it back into the same arguments. A copy of `navo-shell`'s
//! `helper_job/cmdline.rs`; a shared crate follows when a third copy appears.

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
