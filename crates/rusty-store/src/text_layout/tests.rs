use super::*;

fn edit(text: &str, change: impl FnOnce(&str) -> String) -> String {
    keep_layout(text, |plain| Ok::<_, ()>(change(plain))).unwrap()
}

#[test]
fn a_crlf_file_stays_crlf() {
    let after = edit("a = 1\r\nb = 2\r\n", |plain| format!("{plain}c = 3\n"));
    assert_eq!(after, "a = 1\r\nb = 2\r\nc = 3\r\n");
}

#[test]
fn an_lf_file_stays_lf() {
    let after = edit("a = 1\nb = 2\n", |plain| format!("{plain}c = 3\n"));
    assert_eq!(after, "a = 1\nb = 2\nc = 3\n");
}

#[test]
fn a_mixed_file_keeps_each_line_ending_and_new_lines_take_the_first_one() {
    let before = "a = 1\r\nb = 2\nc = 3\r\n";
    let after = edit(before, |plain| plain.replace("b = 2\n", "b = 2\nnew = 0\n"));
    assert_eq!(after, "a = 1\r\nb = 2\nnew = 0\r\nc = 3\r\n");
    let lf_first = "a = 1\nb = 2\r\n";
    let after = edit(lf_first, |plain| format!("{plain}c = 3\n"));
    assert_eq!(after, "a = 1\nb = 2\r\nc = 3\n");
}

#[test]
fn a_changed_line_in_a_mixed_file_keeps_its_ending() {
    let before = "# x\n[a]\r\nmode = \"light\"\r\nsize = 1\n";
    let after = edit(before, |plain| plain.replace("light", "dark"));
    assert_eq!(after, "# x\n[a]\r\nmode = \"dark\"\r\nsize = 1\n");
}

#[test]
fn an_unchanged_mixed_file_comes_back_byte_for_byte() {
    let before = "\u{feff}# x\r\n\n[a]\nk = 1\r\n\r\n# end";
    assert_eq!(edit(before, str::to_owned), before);
}
