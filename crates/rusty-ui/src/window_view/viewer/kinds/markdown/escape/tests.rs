use super::escape_raw;

#[test]
fn html_and_math_outside_code_become_literal_text() {
    let cases = [
        ("a <b>bold</b>\n", "a \\<b\\>bold\\</b\\>\n"),
        ("<div>\n", "\\<div\\>\n"),
        ("costs $5 and $6\n", "costs \\$5 and \\$6\n"),
        ("$$\nx\n$$\n", "\\$\\$\nx\n\\$\\$\n"),
        ("a > b\n", "a \\> b\n"),
        ("<https://x.org>\n", "\\<https://x.org\\>\n"),
    ];
    for (source, escaped) in cases {
        assert_eq!(escape_raw(source), escaped, "{source:?}");
    }
}

#[test]
fn quotes_lists_escapes_and_code_keep_their_meaning() {
    let kept = [
        "> quoted\n> > twice\n",
        "- item\n  1. inner\n",
        "already \\<escaped\\> and \\$\n",
        "inline `a < b` and ``x ` <y>`` code\n",
        "```html\n<div>$x$</div>\n```\n",
        "~~~\n<b>\n~~~\n",
        "Code:\n\n    if a < b { $x }\n",
    ];
    for source in kept {
        assert_eq!(escape_raw(source), source, "{source:?}");
    }
    assert_eq!(escape_raw("> <b>\n"), "> \\<b\\>\n");
    assert_eq!(escape_raw("\\`<b>`\n"), "\\`\\<b\\>`\n");
    assert_eq!(escape_raw("`open <b>\n"), "`open \\<b\\>\n");
    assert_eq!(escape_raw("- a\n\n    <b>\n"), "- a\n\n    \\<b\\>\n");
}
