pub(super) const MAX_NESTING: usize = 1_000;

fn emphasis_run(text: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for byte in text.bytes() {
        if matches!(byte, b'*' | b'_' | b'~') {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

fn bracket_depth(text: &str) -> usize {
    let mut deepest = 0;
    let mut depth = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            depth = 0;
        }
        for byte in line.bytes() {
            match byte {
                b'[' => {
                    depth += 1;
                    deepest = deepest.max(depth);
                }
                b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    deepest
}

fn marker_len(rest: &[u8]) -> Option<usize> {
    let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    let marker = match rest.get(digits) {
        Some(b'.' | b')') if (1..=9).contains(&digits) => digits + 1,
        Some(b'-' | b'+' | b'*') if digits == 0 => 1,
        _ => return None,
    };
    matches!(rest.get(marker), Some(b' ' | b'\t')).then_some(marker)
}

fn container_width(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    let mut width = 0;
    while let Some(byte) = bytes.get(at) {
        let step = match byte {
            b' ' | b'>' => 1,
            b'\t' => 4,
            _ => match marker_len(&bytes[at..]) {
                Some(len) => len,
                None => break,
            },
        };
        width += step;
        at += if *byte == b'\t' { 1 } else { step };
    }
    width
}

pub(super) fn is_too_deep(text: &str) -> bool {
    emphasis_run(text) > MAX_NESTING
        || bracket_depth(text) > MAX_NESTING
        || text.lines().any(|line| container_width(line) > MAX_NESTING)
}

#[cfg(test)]
mod tests {
    use super::{MAX_NESTING, is_too_deep};

    fn nested(prefix: &str, levels: usize, body: &str) -> String {
        format!("{}{body}\n", prefix.repeat(levels))
    }

    #[test]
    fn each_kind_of_nesting_over_1_000_levels_is_too_deep() {
        let over = MAX_NESTING + 1;
        let cases = [
            format!("{}x{}", "*".repeat(over), "*".repeat(over)),
            format!("{}x", "_".repeat(over)),
            format!("{}x", "~".repeat(over)),
            nested(">", over, "x"),
            nested("> ", over, "x"),
            nested("[", over, "x"),
            nested("- ", over, "x"),
            nested("1. ", over, "x"),
            nested(" ", over, "- x"),
            nested("\t", over / 4 + 1, "- x"),
        ];
        for text in cases {
            assert!(is_too_deep(&text), "{:?}", &text[..12]);
        }
    }

    #[test]
    fn ordinary_documents_and_1_000_levels_are_rendered() {
        let fine = [
            "# Notes\n\n- one\n  - two\n    - three\n\n> quoted **bold** [link](x)\n".to_owned(),
            nested(">", MAX_NESTING, "x"),
            format!("{}\n\n{}", "[".repeat(900), "[".repeat(900)),
            "***\n---\n".repeat(5_000),
        ];
        for text in fine {
            assert!(!is_too_deep(&text), "{:?}", &text[..12]);
        }
    }

    #[test]
    fn the_scan_of_hostile_input_fits_a_1_mib_stack() {
        let text = format!("{}x{}", "*".repeat(50_000), "*".repeat(50_000));
        let worker = std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || is_too_deep(&text))
            .expect("a 1 MiB thread starts");
        assert!(worker.join().expect("the scan does not overflow"));
    }
}
