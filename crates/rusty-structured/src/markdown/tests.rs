use super::{
    Align, Block, BlockKind, Document, MAX_BLOCK_TEXT, MAX_BLOCKS, MAX_DEPTH, MAX_SOURCE, Marker,
    Refusal, Style, TABLES_AS_TEXT, parse,
};

fn doc(source: &str) -> Document {
    parse(source).expect("the source renders")
}

fn texts(doc: &Document) -> Vec<&str> {
    doc.blocks().iter().map(|block| doc.text(block)).collect()
}

fn only(doc: &Document) -> Block {
    assert_eq!(doc.blocks().len(), 1, "{:?}", texts(doc));
    doc.blocks()[0]
}

fn styled(doc: &Document, block: &Block, style: Style) -> Vec<String> {
    let text = doc.text(block);
    let mut at = 0;
    let mut found = Vec::new();
    for span in doc.spans(block) {
        let end = at + span.len();
        if span.style().has(style) {
            found.push(text[at..end].to_owned());
        }
        at = end;
    }
    found
}

#[test]
fn headings_keep_their_level_and_lose_their_hashes() {
    let doc = doc("# One\n\n### Three\n\nSetext\n------\n");
    let kinds: Vec<BlockKind> = doc.blocks().iter().map(|block| block.kind).collect();
    assert_eq!(
        kinds,
        [
            BlockKind::Heading(1),
            BlockKind::Heading(3),
            BlockKind::Heading(2)
        ]
    );
    assert_eq!(texts(&doc), ["One", "Three", "Setext"]);
}

#[test]
fn a_paragraph_joins_soft_breaks_with_a_space_and_keeps_hard_breaks() {
    let doc = doc("one\ntwo  \nthree\n");
    let block = only(&doc);
    assert_eq!(block.kind, BlockKind::Paragraph);
    assert_eq!(doc.text(&block), "one two\nthree");
}

#[test]
fn inline_styles_become_flat_spans() {
    let doc = doc("a **b *c*** ~~d~~ `e` [f](https://x.org)\n");
    let block = only(&doc);
    assert_eq!(doc.text(&block), "a b c d e f");
    assert_eq!(styled(&doc, &block, Style::BOLD), ["b ", "c"]);
    assert_eq!(styled(&doc, &block, Style::ITALIC), ["c"]);
    assert_eq!(styled(&doc, &block, Style::STRIKE), ["d"]);
    assert_eq!(styled(&doc, &block, Style::CODE), ["e"]);
    let link = doc
        .spans(&block)
        .iter()
        .find_map(|span| span.link())
        .expect("a link span");
    assert_eq!(
        doc.link(link).map(|link| link.target()),
        Some("https://x.org")
    );
    assert_eq!(doc.links().len(), 1);
}

#[test]
fn neighbouring_text_with_the_same_style_is_one_span() {
    let doc = doc("plain text &amp; more plain text\n");
    let block = only(&doc);
    assert_eq!(doc.text(&block), "plain text & more plain text");
    assert_eq!(doc.spans(&block).len(), 1);
}

#[test]
fn quotes_are_a_depth_on_each_block() {
    let doc = doc("> a\n>\n> > b\n\nc\n");
    let depths: Vec<u8> = doc.blocks().iter().map(|block| block.quote).collect();
    assert_eq!(depths, [1, 2, 0]);
    assert_eq!(texts(&doc), ["a", "b", "c"]);
}

#[test]
fn list_items_carry_depth_and_marker() {
    let doc = doc("- a\n- b\n  1. c\n  2. d\n- [x] done\n- [ ] open\n");
    let items: Vec<(u8, Option<Marker>)> = doc
        .blocks()
        .iter()
        .map(|block| (block.list, block.marker))
        .collect();
    assert_eq!(
        items,
        [
            (1, Some(Marker::Bullet)),
            (1, Some(Marker::Bullet)),
            (2, Some(Marker::Number(1))),
            (2, Some(Marker::Number(2))),
            (1, Some(Marker::Task(true))),
            (1, Some(Marker::Task(false))),
        ]
    );
    assert_eq!(texts(&doc), ["a", "b", "c", "d", "done", "open"]);
}

#[test]
fn a_loose_item_with_two_paragraphs_marks_only_the_first() {
    let doc = doc("1. first\n\n   second\n");
    let markers: Vec<Option<Marker>> = doc.blocks().iter().map(|block| block.marker).collect();
    assert_eq!(markers, [Some(Marker::Number(1)), None]);
    assert!(doc.blocks().iter().all(|block| block.list == 1));
}

#[test]
fn an_empty_item_still_shows_its_marker() {
    let doc = doc("-\n- b\n");
    assert_eq!(texts(&doc), ["", "b"]);
    assert_eq!(doc.blocks()[0].marker, Some(Marker::Bullet));
}

#[test]
fn code_blocks_keep_language_lines_and_literal_text() {
    let doc = doc("```rust\nif a < b {\n    x\n}\n```\n\n    indented <b>\n");
    assert_eq!(texts(&doc), ["if a < b {\n    x\n}", "indented <b>"]);
    let first = doc.blocks()[0];
    let BlockKind::Code { language } = first.kind else {
        panic!("a code block: {first:?}");
    };
    assert_eq!(language.and_then(|at| doc.label(at)), Some("rust"));
    assert_eq!(doc.blocks()[1].kind, BlockKind::Code { language: None });
}

#[test]
fn tables_become_rows_with_cells_and_alignment() {
    let doc = doc("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 |\n| x | y | z |\n");
    assert_eq!(doc.blocks().len(), 3);
    let head = doc.blocks()[0];
    let BlockKind::Row { table, is_head } = head.kind else {
        panic!("a table row: {head:?}");
    };
    assert!(is_head);
    let table = doc.table(table).expect("the table");
    assert_eq!(
        table.alignments(),
        [Align::Left, Align::Center, Align::Right]
    );
    let cells: Vec<u8> = doc
        .spans(&doc.blocks()[1])
        .iter()
        .map(|span| span.cell())
        .collect();
    assert_eq!(cells, [0, 1]);
    assert_eq!(doc.text(&doc.blocks()[2]), "xyz");
}

#[test]
fn padded_rows_keep_only_the_cells_they_have() {
    let padded = "|a".repeat(100) + "|\n" + &"|-".repeat(100) + "|\n" + &"x\n".repeat(1_000);
    let doc = doc(&padded);
    assert_eq!(doc.blocks().len(), 1_001);
    let row = doc.blocks()[1];
    assert_eq!(
        doc.spans(&row).len(),
        1,
        "{:?} {:?}",
        doc.spans(&row),
        doc.text(&row)
    );
    assert!(doc.spans_total() < 2_000);
}

#[test]
fn a_table_wider_than_the_column_cap_ends_with_an_ellipsis() {
    let wide = "|a".repeat(100) + "|\n" + &"|-".repeat(100) + "|\n";
    let doc = doc(&wide);
    let head = doc.blocks()[0];
    let last = doc.spans(&head).last().copied().expect("cells");
    assert_eq!(usize::from(last.cell()), super::MAX_COLUMNS - 1);
    assert!(doc.text(&head).ends_with('…'));
}

#[test]
fn rules_images_and_footnotes_have_their_own_shape() {
    let source = "a[^n]\n\n***\n\n![The logo](logo.png)\n\n[^n]: The note.\n";
    let doc = doc(source);
    let kinds: Vec<BlockKind> = doc.blocks().iter().map(|block| block.kind).collect();
    assert_eq!(kinds[1], BlockKind::Rule);
    assert_eq!(doc.text(&doc.blocks()[0]), "a[n]");
    let image = doc.blocks()[2];
    assert_eq!(doc.text(&image), "Image: The logo");
    assert_eq!(styled(&doc, &image, Style::IMAGE), ["Image: The logo"]);
    let link = doc.spans(&image)[0].link().expect("the image target");
    assert_eq!(doc.link(link).map(|link| link.target()), Some("logo.png"));
    let note = doc.blocks()[3];
    assert_eq!(doc.text(&note), "The note.");
    let Some(Marker::Footnote(label)) = note.marker else {
        panic!("a footnote marker: {note:?}");
    };
    assert_eq!(doc.label(label), Some("n"));
}

#[test]
fn html_and_dollars_are_literal_muted_text() {
    let doc = doc("<div>\n<b>x</b>\n</div>\n\na <i>b</i> $x$ and $$y$$\n");
    let html = doc.blocks()[0];
    assert_eq!(html.kind, BlockKind::Html);
    assert_eq!(doc.text(&html), "<div>\n<b>x</b>\n</div>");
    let line = doc.blocks()[1];
    assert_eq!(doc.text(&line), "a <i>b</i> $x$ and $$y$$");
    assert_eq!(styled(&doc, &line, Style::HTML), ["<i>", "</i>"]);
}

#[test]
fn autolinks_are_links() {
    let doc = doc("<https://example.com> and <admin@example.com>\n");
    let targets: Vec<&str> = doc.links().iter().map(|link| link.target()).collect();
    assert_eq!(targets, ["https://example.com", "mailto:admin@example.com"]);
}

#[test]
fn crlf_and_lone_cr_are_line_breaks() {
    let doc = doc("# a\r\n\r\n- b\r- c\r");
    assert_eq!(texts(&doc), ["a", "b", "c"]);
}

#[test]
fn depth_is_clamped_for_display() {
    let source = ">".repeat(20) + " x\n";
    assert_eq!(only(&doc(&source)).quote, MAX_DEPTH);
    let list: String = (0..20)
        .map(|depth| format!("{}- x\n", "  ".repeat(depth)))
        .collect();
    let doc = doc(&list);
    assert_eq!(doc.blocks().last().map(|block| block.list), Some(MAX_DEPTH));
}

#[test]
fn long_text_is_split_into_continued_blocks() {
    let source = "word ".repeat(3_000) + "\n";
    let doc = doc(&source);
    assert!(doc.blocks().len() > 1);
    assert!(!doc.blocks()[0].is_continued);
    assert!(doc.blocks()[1..].iter().all(|block| block.is_continued));
    for block in doc.blocks() {
        assert!(doc.text(block).len() <= MAX_BLOCK_TEXT);
    }
    let joined: String = texts(&doc).concat();
    assert_eq!(joined, source.trim_end());
}

#[test]
fn tabs_and_control_characters_never_reach_the_text() {
    let doc = doc("```\n\ta\u{1}b\n```\n");
    assert_eq!(doc.text(&only(&doc)), "    a b");
}

#[test]
fn files_over_1_mib_are_refused() {
    assert_eq!(MAX_SOURCE, 1024 * 1024);
    let big = "a".repeat(MAX_SOURCE + 1);
    assert_eq!(parse(&big).err(), Some(Refusal::TooLarge));
}

#[test]
fn too_many_blocks_are_refused() {
    let many = "- a\n".repeat(MAX_BLOCKS + 1);
    assert_eq!(parse(&many).err(), Some(Refusal::TooManyBlocks));
}

#[test]
fn a_block_and_a_span_stay_small() {
    assert!(std::mem::size_of::<Block>() <= 40);
    assert!(std::mem::size_of::<super::Span>() <= 12);
}

fn padded_tables(count: usize) -> String {
    let one = "|a".repeat(362) + "|\n" + &"|-".repeat(362) + "|\n" + &"x\n".repeat(724) + "\n";
    one.repeat(count)
}

#[test]
fn tables_over_a_million_cells_in_the_document_show_as_text() {
    let cut = doc(&padded_tables(4));
    assert_eq!(cut.notes(), [TABLES_AS_TEXT]);
    assert!(
        cut.blocks()
            .iter()
            .all(|block| !matches!(block.kind, BlockKind::Row { .. }))
    );
    assert!(doc(&padded_tables(3)).notes().is_empty());
}

#[test]
fn quoted_tables_count_towards_the_cell_total() {
    let quoted: String = padded_tables(4)
        .lines()
        .map(|line| format!("> {line}\n"))
        .collect();
    assert_eq!(doc(&quoted).notes(), [TABLES_AS_TEXT]);
}

#[test]
fn over_2_000_footnote_lines_are_refused_wherever_they_sit() {
    let shapes = [
        "[^a]: b\n".repeat(2_001),
        " > [^a]: b\n".repeat(2_001),
        "- [^a]: b\n".repeat(2_001),
        "1. [^a]: b\n".repeat(2_001),
        "```\n".to_owned() + &"[^a]: b\n".repeat(2_001) + "```\n",
    ];
    for shape in shapes {
        assert_eq!(
            parse(&shape).err(),
            Some(Refusal::TooManyFootnotes),
            "{:?}",
            &shape[..12]
        );
    }
    assert!(parse(&"[^a]: b\n".repeat(2_000)).is_ok());
}

fn chained_tables(count: usize, head: &str) -> String {
    let one = "# h\n".to_owned()
        + &"|a".repeat(362)
        + "|\n"
        + &"|-".repeat(362)
        + "|\n"
        + &"x\n".repeat(724);
    head.to_owned() + &one.repeat(count)
}

#[test]
fn tables_joined_without_blank_lines_count_as_one_run() {
    assert_eq!(
        doc(&chained_tables(4, "|a|\n|-|\nx\n")).notes(),
        [TABLES_AS_TEXT]
    );
    assert_eq!(
        doc(&chained_tables(4, "```\n|-|\n```\n")).notes(),
        [TABLES_AS_TEXT]
    );
}

#[test]
fn rows_of_non_breaking_spaces_are_not_blank() {
    let one = "|a".repeat(443) + "|\n" + &"|-".repeat(443) + "|\n" + &"\u{a0}\n".repeat(591) + "\n";
    assert_eq!(doc(&one.repeat(4)).notes(), [TABLES_AS_TEXT]);
}

fn large_readme() -> String {
    let section =
        "## Section\n\nSome prose with **bold** text and a [link](https://example.com).\n\n\
        | Name | Size | Date | Note |\n|:-----|-----:|:----:|------|\n"
            .to_owned()
            + &"| a.txt | 3 | 2026-10-16 | fine |\n".repeat(20)
            + "\n- one\n- two\n\n```rust\nlet x = 1;\n```\n\n---\n\n";
    section.repeat((MAX_SOURCE - 1024) / section.len())
}

#[test]
fn a_large_readme_with_many_small_tables_keeps_tables_on() {
    let doc = doc(&large_readme());
    assert!(doc.notes().is_empty(), "{:?}", doc.notes());
    assert!(
        doc.blocks()
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Row { .. }))
    );
}

#[test]
fn an_item_that_opens_with_a_list_keeps_both_markers() {
    let doc = doc("- - a\n1. - b\n");
    let items: Vec<(&str, u8, Option<Marker>)> = doc
        .blocks()
        .iter()
        .map(|block| (doc.text(block), block.list, block.marker))
        .collect();
    assert_eq!(
        items,
        [
            ("", 1, Some(Marker::Bullet)),
            ("a", 2, Some(Marker::Bullet)),
            ("", 1, Some(Marker::Number(1))),
            ("b", 2, Some(Marker::Bullet)),
        ]
    );
}

fn column_words(doc: &Document) -> Vec<(String, bool)> {
    (0..doc.table(0).expect("a table").widths().len())
        .map(|column| {
            let word = doc.column_word(0, column).expect("a word");
            (word.text.to_owned(), word.is_mono)
        })
        .collect()
}

#[test]
fn each_table_column_knows_its_longest_word() {
    let source = "| Tier | Used for |\n| --- | --- |\n| **T1** | Moves, `formatted` edits |\n";
    assert_eq!(
        column_words(&doc(source)),
        [("Tier".to_owned(), false), ("formatted".to_owned(), true)]
    );
}

#[test]
fn a_column_word_is_cut_to_40_characters() {
    let long = "x".repeat(100);
    let source = format!("| a |\n| - |\n| {long} |\n");
    assert_eq!(column_words(&doc(&source)), [("x".repeat(40), false)]);
}

#[test]
fn an_empty_column_has_an_empty_word() {
    assert_eq!(
        column_words(&doc("| a | |\n| - | - |\n")),
        [("a".to_owned(), false), (String::new(), false)]
    );
}

#[test]
fn every_block_knows_the_source_line_it_starts_on() {
    let source = "# A\n\npara\nline two\n\n- item\n\n| a |\n|---|\n| b |\n\n```\ncode\n```\n";
    let doc = doc(source);
    let lines: Vec<Option<u32>> = (0..doc.blocks().len()).map(|at| doc.line(at)).collect();
    assert_eq!(
        lines,
        [Some(0), Some(2), Some(5), Some(7), Some(9), Some(11)]
    );
}

#[test]
fn a_source_line_maps_to_the_block_that_holds_it() {
    let doc = doc("# A\r\n\r\npara\r\nline two\r\n\r\n- item\r\n");
    let found: Vec<Option<usize>> = (0..7).map(|line| doc.block_at_line(line)).collect();
    assert_eq!(
        found,
        [
            Some(0),
            Some(0),
            Some(1),
            Some(1),
            Some(1),
            Some(2),
            Some(2)
        ]
    );
    assert_eq!(super::parse("").expect("empty").block_at_line(3), None);
}
