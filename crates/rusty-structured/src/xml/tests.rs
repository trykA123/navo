use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use super::{attach_children, build_index, parse_xml, scan_children};
use crate::{Key, NodeId, NodeKind, Outline, Span, Step, Tree, Watch};

const SMALL: &str = r#"<?xml version="1.0"?>
<root version="2">
  <item id="x">one &amp; only</item>
  <item>two</item>
  <group><item/></group>
  <!-- note -->
</root>"#;

fn find(tree: &Tree, path: &str) -> NodeId {
    tree.ids()
        .find(|id| tree.path(*id) == path)
        .unwrap_or_else(|| panic!("no node at {path}"))
}

#[test]
fn small_xml_becomes_a_tree_with_attributes_text_and_paths() {
    let tree = parse_xml(SMALL.as_bytes()).unwrap();
    assert_eq!(tree.len(), 5);
    let root = tree.root().unwrap();
    assert_eq!(
        (tree.key(root), tree.kind(root)),
        (Key::Name("root"), NodeKind::Element)
    );
    assert_eq!(tree.attrs(root), "version=\"2\"");
    assert_eq!(tree.child_count(root), 3);
    let first = find(&tree, "/root/item[1]");
    assert_eq!(
        (tree.value(first), tree.attrs(first)),
        ("one & only", "id=\"x\"")
    );
    assert_eq!(tree.value(find(&tree, "/root/item[2]")), "two");
    assert_eq!(
        tree.path(find(&tree, "/root/group/item")),
        "/root/group/item"
    );
    assert_eq!(tree.value(root), "");
}

#[test]
fn malformed_xml_reports_line_and_column() {
    let error = parse_xml(b"<a>\n  <b></a>").unwrap_err();
    assert_eq!(error.line, 2, "{error}");
    assert!(error.column > 1, "{error}");
    let error = parse_xml(b"<a>\n<b>").unwrap_err();
    assert!(error.message.contains("never closed"), "{error}");
    assert!(parse_xml(b"<a/><b/>").is_err());
    assert!(parse_xml(b"  ").is_err());
}

#[test]
fn truncated_xml_never_panics() {
    for end in 0..SMALL.len() {
        let _ = parse_xml(&SMALL.as_bytes()[..end]);
    }
}

const PACKAGES: usize = 4;

fn write_big(path: &std::path::Path, target: u64) -> (u64, Vec<u32>) {
    let mut out = BufWriter::new(File::create(path).unwrap());
    writeln!(out, "<?xml version=\"1.0\"?>\n<AUTOSAR>").unwrap();
    let per_package = target / PACKAGES as u64;
    let mut elements = 1u64;
    let mut counts = Vec::new();
    let mut written = 0u64;
    for package in 0..PACKAGES {
        writeln!(out, "<AR-PACKAGE UUID=\"p{package}\">").unwrap();
        elements += 1;
        let start = written;
        let mut count = 0u32;
        while written - start < per_package {
            let line = format!(
                "  <ELEMENT N=\"{count}\"><SHORT-NAME>Signal{count}</SHORT-NAME><LENGTH>{}</LENGTH></ELEMENT>\n",
                count % 64
            );
            out.write_all(line.as_bytes()).unwrap();
            written += line.len() as u64;
            elements += 3;
            count += 1;
        }
        out.write_all(b"</AR-PACKAGE>\n").unwrap();
        counts.push(count);
    }
    out.write_all(b"</AUTOSAR>\n").unwrap();
    out.flush().unwrap();
    (elements, counts)
}

type Reads = Arc<Mutex<Vec<(u64, usize)>>>;

fn reader_of(
    path: &std::path::Path,
    reads: &Reads,
) -> impl FnMut(u64, usize) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).unwrap();
    let reads = reads.clone();
    move |offset, len| {
        reads.lock().unwrap().push((offset, len));
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity(len);
        (&mut file)
            .take(len as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        Ok(bytes)
    }
}

#[test]
fn a_50_mb_xml_index_stays_small_and_expanding_reads_only_that_range() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.arxml");
    let (elements, counts) = write_big(&path, 50 * 1024 * 1024);
    let len = std::fs::metadata(&path).unwrap().len();
    assert!(len > 50 * 1024 * 1024);
    let reads = Reads::default();
    let mut read = reader_of(&path, &reads);
    let cancel = AtomicBool::new(false);
    let mut seen = Vec::new();
    let mut progress = |count: u64| seen.push(count);
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let index = build_index(&mut read, len, &mut watch).unwrap().unwrap();
    assert_eq!(index.elements, elements);
    assert!(seen.len() > 10 && seen.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(index.size_bytes() < 16 * 1024, "{}", index.size_bytes());
    assert_eq!(index.root.name, "AUTOSAR");
    let children: Vec<u32> = index.top.iter().map(|entry| entry.children).collect();
    assert_eq!(children, counts);
    assert!(
        reads
            .lock()
            .unwrap()
            .iter()
            .all(|(_, len)| *len <= 1024 * 1024)
    );

    expand_the_second_package(&index.tree(), &mut read, &reads, &counts);
}

fn expand_the_second_package(
    tree: &Tree,
    read: &mut impl FnMut(u64, usize) -> Result<Vec<u8>, String>,
    reads: &Reads,
    counts: &[u32],
) {
    let mut tree = tree.clone();
    let root = tree.root().unwrap();
    assert_eq!(tree.child_count(root), PACKAGES as u32);
    let second = tree.children(root).nth(1).unwrap();
    assert_eq!(tree.attrs(second), "UUID=\"p1\"");
    let lazy = tree.lazy(second).unwrap();
    assert_eq!(lazy.count, counts[1]);

    reads.lock().unwrap().clear();
    let entries = scan_children(read, lazy.span).unwrap();
    let reads_seen = reads.lock().unwrap().clone();
    assert!(!reads_seen.is_empty());
    for (offset, len) in reads_seen {
        assert!(offset >= lazy.span.start && offset + len as u64 <= lazy.span.end);
    }
    assert_eq!(entries.len(), counts[1] as usize);
    assert_eq!(entries[3].name, "ELEMENT");
    assert_eq!(entries[3].attrs, "N=\"3\"");
    assert_eq!(entries[3].children, 2);

    attach_children(&mut tree, second, &entries);
    assert!(tree.lazy(second).is_none());
    let fourth = tree.children(second).nth(3).unwrap();
    assert_eq!(tree.path(fourth), "/AUTOSAR/AR-PACKAGE[2]/ELEMENT[4]");
    let short = tree.lazy(fourth).unwrap();
    let names = scan_children(read, short.span).unwrap();
    assert_eq!(names[0].text, "Signal3");
}

#[test]
fn lazy_nodes_ask_to_load_when_expanded() {
    let bytes = SMALL.as_bytes().to_vec();
    let mut read = |offset: u64, len: usize| {
        let start = offset as usize;
        Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
    };
    let cancel = AtomicBool::new(false);
    let mut progress = |_: u64| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let len = SMALL.len() as u64;
    let index = build_index(&mut read, len, &mut watch).unwrap().unwrap();
    let mut tree = index.tree();
    let mut outline = Outline::new(&tree);
    assert_eq!(outline.rows().len(), 4);
    outline.move_by(3);
    let Step::Load(node, lazy) = outline.right(&tree) else {
        panic!("a lazy group asks to load");
    };
    let entries = scan_children(&mut read, lazy.span).unwrap();
    attach_children(&mut tree, node, &entries);
    outline.loaded(&tree, node);
    assert_eq!(outline.rows().len(), 5);
    assert_eq!(tree.path(outline.rows()[4].node), "/root/group/item");
}

#[test]
fn a_cancelled_index_returns_nothing_and_bad_xml_reports_its_line() {
    let mut text = String::from("<a>\n");
    for at in 0..20_000 {
        text.push_str(&format!("<b n=\"{at}\"/>\n"));
    }
    let bytes = text.into_bytes();
    let len = bytes.len() as u64;
    let mut read = |offset: u64, size: usize| {
        let start = offset as usize;
        Ok(bytes[start..(start + size).min(bytes.len())].to_vec())
    };
    let cancel = AtomicBool::new(true);
    let mut progress = |_: u64| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    assert!(build_index(&mut read, len, &mut watch).unwrap().is_none());
    let broken = b"<a>\n<b>\n</c>\n</a>".to_vec();
    let mut read = |offset: u64, size: usize| {
        let start = offset as usize;
        Ok(broken[start..(start + size).min(broken.len())].to_vec())
    };
    let cancel = AtomicBool::new(false);
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let error = build_index(&mut read, 15, &mut watch).unwrap_err();
    assert_eq!(error.line, 3, "{error}");
    let _ = Span { start: 0, end: 0 };
}
