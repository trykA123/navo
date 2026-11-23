//! Builds a Windows command line from a program and its arguments, quoted so the child's C
//! runtime splits it back into the same arguments (S2a1; `spawn_win.rs` passes it to `CreateProcessW`).

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

#[cfg(test)]
mod tests {
    use super::command_line;

    /// Builds the command line from `&str` parts and returns it as a `String`.
    fn line(program: &str, args: &[&str]) -> String {
        let program: Vec<u16> = program.encode_utf16().collect();
        let args: Vec<Vec<u16>> = args
            .iter()
            .map(|arg| arg.encode_utf16().collect())
            .collect();
        String::from_utf16(&command_line(&program, args.iter().map(Vec::as_slice))).unwrap()
    }

    #[test]
    fn plain_arguments_stay_bare_and_the_program_is_always_quoted() {
        assert_eq!(
            line(r"C:\Program Files\navo.exe", &["--pdf-helper"]),
            r#""C:\Program Files\navo.exe" --pdf-helper"#
        );
    }

    #[test]
    fn spaces_tabs_and_empty_arguments_are_quoted() {
        assert_eq!(
            line("a", &["b c", "d\te", ""]),
            "\"a\" \"b c\" \"d\te\" \"\""
        );
    }

    #[test]
    fn quotes_and_the_backslashes_before_them_are_escaped() {
        assert_eq!(line("a", &[r#"say "hi""#]), r#""a" "say \"hi\"""#);
        assert_eq!(line("a", &[r#"x\"y"#]), r#""a" "x\\\"y""#);
    }

    #[test]
    fn trailing_backslashes_are_doubled_only_inside_quotes() {
        assert_eq!(line("a", &[r"C:\dir\"]), r#""a" C:\dir\"#);
        assert_eq!(line("a", &[r"C:\my dir\"]), r#""a" "C:\my dir\\""#);
        assert_eq!(line("a", &[r"a\\b"]), r#""a" a\\b"#);
    }
}
