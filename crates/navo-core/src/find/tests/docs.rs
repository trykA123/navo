use crate::find::{DocsError, xml_text};

fn text(xml: &str) -> String {
    let mut out = String::new();
    xml_text(xml.as_bytes(), &mut out, 1 << 20).unwrap();
    out
}

fn error(xml: &[u8], limit: usize) -> DocsError {
    xml_text(xml, &mut String::new(), limit).unwrap_err()
}

#[test]
fn word_paragraphs_end_with_newlines() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?><w:document><w:body><w:p><w:r><w:t xml:space="preserve">Hello </w:t></w:r><w:r><w:t>world</w:t></w:r></w:p><w:p><w:r><w:t>Two</w:t><w:br/><w:t>lines</w:t></w:r></w:p></w:body></w:document>"#;
    assert_eq!(text(xml), "Hello world\nTwo\nlines\n");
}

#[test]
fn other_paragraph_tags_end_with_newlines() {
    let xml = "<a:p>slide</a:p><text:h>Title</text:h><text:p>body</text:p><si>cell</si><h2>h</h2><li>item</li><p>x<br/>y</p>";
    assert_eq!(text(xml), "slide\nTitle\nbody\ncell\nh\nitem\nx\ny\n");
}

#[test]
fn entities_decode_and_unknown_ones_stay() {
    assert_eq!(
        text(
            "<t>&lt;a&gt; &amp; &quot;b&quot; &apos;c&apos; &#65;&#x42;&#x1F600; &nbsp; & x &#xZZ;</t>"
        ),
        "<a> & \"b\" 'c' AB\u{1F600} &nbsp; & x &#xZZ;"
    );
}

#[test]
fn cdata_is_kept_and_comments_and_instructions_skipped() {
    assert_eq!(
        text("<t>a<!-- <p>no</p> --><![CDATA[<raw> &amp;]]><?pi skip?>b</t>"),
        "a<raw> &amp;b"
    );
}

#[test]
fn attributes_are_not_text() {
    assert_eq!(text(r#"<t a="x > y" b='<p>'>in</t>"#), "in");
}

#[test]
fn bom_rules() {
    let mut out = String::new();
    xml_text(b"\xEF\xBB\xBF<t>hi</t>", &mut out, 100).unwrap();
    assert_eq!(out, "hi");
    assert_eq!(error(b"\xFF\xFE<\0t\0>\0", 100), DocsError::Encoding);
    assert_eq!(error(b"\xFE\xFF\0<\0t\0>", 100), DocsError::Encoding);
    assert_eq!(error(b"<t>\xC3</t>", 100), DocsError::Encoding);
}

#[test]
fn billion_laughs_is_refused() {
    let xml = br#"<?xml version="1.0"?>
<!DOCTYPE lolz [
 <!ENTITY lol "lol">
 <!ENTITY lol1 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
 <!ENTITY lol2 "&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;">
]>
<lolz>&lol2;</lolz>"#;
    assert_eq!(error(xml, 1 << 20), DocsError::Dtd);
}

#[test]
fn output_over_the_limit_is_too_large() {
    assert_eq!(error(b"<t>12345</t>", 4), DocsError::TooLarge);
    let mut out = String::from("kept ");
    xml_text(b"<t>1234</t>", &mut out, 4).unwrap();
    assert_eq!(out, "kept 1234");
}

#[test]
fn unterminated_markup_stops_quietly() {
    assert_eq!(text("<t>a</t><unclosed attr=\"x"), "a");
    assert_eq!(text("a<!-- open"), "a");
}
