use rusty_structured::markdown::{BlockKind, Marker, Style, parse};

const README: &str = include_str!("fixtures/README.md");

#[test]
fn a_readme_renders_every_block_type() {
    let doc = parse(README).expect("the README renders");
    let has_kind =
        |wanted: fn(&BlockKind) -> bool| doc.blocks().iter().any(|block| wanted(&block.kind));
    assert!(has_kind(|kind| *kind == BlockKind::Heading(1)));
    assert!(has_kind(|kind| *kind == BlockKind::Heading(2)));
    assert!(has_kind(|kind| *kind == BlockKind::Paragraph));
    assert!(has_kind(|kind| *kind == BlockKind::Rule));
    assert!(has_kind(|kind| *kind == BlockKind::Html));
    assert!(has_kind(|kind| matches!(
        kind,
        BlockKind::Code { language: Some(_) }
    )));
    assert!(has_kind(|kind| matches!(
        kind,
        BlockKind::Code { language: None }
    )));
    assert!(has_kind(|kind| matches!(
        kind,
        BlockKind::Row { is_head: true, .. }
    )));
    assert!(has_kind(|kind| matches!(
        kind,
        BlockKind::Row { is_head: false, .. }
    )));
}

#[test]
fn a_readme_keeps_list_markers_and_depths() {
    let doc = parse(README).expect("the README renders");
    let blocks = doc.blocks();
    let has_marker = |wanted: Marker| blocks.iter().any(|block| block.marker == Some(wanted));
    for marker in [
        Marker::Bullet,
        Marker::Number(1),
        Marker::Task(true),
        Marker::Task(false),
    ] {
        assert!(has_marker(marker), "{marker:?}");
    }
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.marker, Some(Marker::Footnote(_))))
    );
    assert!(blocks.iter().any(|block| block.quote == 2));
    assert!(blocks.iter().any(|block| block.list == 2));
}

#[test]
fn a_readme_keeps_every_inline_style_and_link() {
    let doc = parse(README).expect("the README renders");
    let styles: Vec<Style> = doc
        .blocks()
        .iter()
        .flat_map(|block| doc.spans(block))
        .map(|span| span.style())
        .collect();
    for style in [
        Style::BOLD,
        Style::ITALIC,
        Style::STRIKE,
        Style::CODE,
        Style::HTML,
        Style::IMAGE,
        Style::FOOTNOTE,
    ] {
        assert!(styles.iter().any(|found| found.has(style)), "{style:?}");
    }
    assert_eq!(doc.links().len(), 2);
}
