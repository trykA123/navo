use std::io::{Cursor, Write};

use navo_core::find::Skip;
use tempfile::TempDir;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use super::{names, search, write};
use crate::CancelToken;
use crate::find::walk::doc_text;

fn zip_with(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(method);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn mark_encrypted(mut bytes: Vec<u8>) -> Vec<u8> {
    let set_flag = |signature: &[u8], offset: usize, data: &mut [u8]| {
        let positions: Vec<usize> = data
            .windows(4)
            .enumerate()
            .filter(|(_, window)| *window == signature)
            .map(|(at, _)| at + offset)
            .collect();
        for at in positions {
            data[at] |= 1;
        }
    };
    set_flag(b"PK\x03\x04", 6, &mut bytes);
    set_flag(b"PK\x01\x02", 8, &mut bytes);
    bytes
}

#[test]
fn docx_hit_found_with_docs_on_and_not_scanned_with_docs_off() {
    let tree = TempDir::new().unwrap();
    let docx = zip_with(
        &[
            (
                "word/document.xml",
                b"<w:document><w:body><w:p><w:r><w:t>Quarterly needle report</w:t></w:r></w:p></w:body></w:document>",
            ),
            (
                "word/footer1.xml",
                b"<w:ftr><w:p><w:r><w:t>footer text</w:t></w:r></w:p></w:ftr>",
            ),
        ],
        CompressionMethod::Deflated,
    );
    write(&tree, "report.docx", &docx);

    let on = search(&tree, "needle docs:on");
    assert_eq!(names(&on), ["report.docx"]);
    let found = on.iter().flat_map(|batch| &batch.files).next().unwrap();
    assert_eq!(found.hits.total, 1);
    assert_eq!(found.hits.hits[0].line, 1);
    assert!(found.hits.hits[0].text.contains("Quarterly needle report"));

    let off = search(&tree, "needle");
    assert!(names(&off).is_empty());
    assert_eq!(off.last().unwrap().progress.skipped, 1);
}

#[test]
fn xlsx_shared_string_hit_is_found() {
    let tree = TempDir::new().unwrap();
    let xlsx = zip_with(
        &[
            (
                "xl/sharedStrings.xml",
                b"<sst><si><t>Budget needle total</t></si></sst>",
            ),
            (
                "xl/worksheets/sheet1.xml",
                b"<worksheet><sheetData/></worksheet>",
            ),
        ],
        CompressionMethod::Deflated,
    );
    write(&tree, "budget.xlsx", &xlsx);

    let batches = search(&tree, "needle docs:on");
    assert_eq!(names(&batches), ["budget.xlsx"]);
    let found = batches
        .iter()
        .flat_map(|batch| &batch.files)
        .next()
        .unwrap();
    assert_eq!(found.hits.total, 1);
}

#[test]
fn epub_pptx_and_odt_hits_are_found() {
    let tree = TempDir::new().unwrap();
    write(
        &tree,
        "book.epub",
        zip_with(
            &[(
                "OEBPS/chapter1.xhtml",
                b"<html><body><p>Chapter needle line</p></body></html>",
            )],
            CompressionMethod::Deflated,
        ),
    );
    write(
        &tree,
        "deck.pptx",
        zip_with(
            &[(
                "ppt/slides/slide1.xml",
                b"<p:sld><a:p><a:r><a:t>Slide needle point</a:t></a:r></a:p></p:sld>",
            )],
            CompressionMethod::Deflated,
        ),
    );
    write(
        &tree,
        "notes.odt",
        zip_with(
            &[(
                "content.xml",
                b"<office:document-content><text:p>ODT needle note</text:p></office:document-content>",
            )],
            CompressionMethod::Deflated,
        ),
    );

    let mut matched = names(&search(&tree, "needle docs:on"));
    matched.sort();
    assert_eq!(matched, ["book.epub", "deck.pptx", "notes.odt"]);
}

#[test]
fn zip_bomb_is_skipped_and_excluded_under_not_on() {
    let tree = TempDir::new().unwrap();
    let zeros = vec![0u8; 2 * 1024 * 1024];
    let bomb = zip_with(
        &[("word/document.xml", &zeros)],
        CompressionMethod::Deflated,
    );
    write(&tree, "bomb.docx", &bomb);

    assert_eq!(
        doc_text(&tree.path().join("bomb.docx"), "docx", &CancelToken::new()),
        Err(Skip::Document)
    );

    let normal = search(&tree, "needle docs:on");
    assert!(names(&normal).is_empty());
    assert_eq!(normal.last().unwrap().progress.skipped, 1);

    let inverted = search(&tree, "needle docs:on not:on");
    assert!(names(&inverted).is_empty());
    assert_eq!(inverted.last().unwrap().progress.skipped, 1);
}

#[test]
fn document_with_600_entries_is_skipped() {
    let tree = TempDir::new().unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for index in 0..599 {
        writer
            .start_file(format!("extra/{index:04}.xml"), options)
            .unwrap();
        writer.write_all(b"<a/>").unwrap();
    }
    writer.start_file("word/document.xml", options).unwrap();
    writer.write_all(b"<w:p><w:t>needle</w:t></w:p>").unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    write(&tree, "many.docx", &bytes);

    assert_eq!(
        doc_text(&tree.path().join("many.docx"), "docx", &CancelToken::new()),
        Err(Skip::Document)
    );

    let batches = search(&tree, "needle docs:on");
    assert!(names(&batches).is_empty());
    assert_eq!(batches.last().unwrap().progress.skipped, 1);
}

#[test]
fn encrypted_corrupt_and_doctype_documents_are_skipped() {
    let tree = TempDir::new().unwrap();
    let encrypted = mark_encrypted(zip_with(
        &[("word/document.xml", b"<w:p><w:t>needle</w:t></w:p>")],
        CompressionMethod::Deflated,
    ));
    let doctype = zip_with(
        &[(
            "word/document.xml",
            b"<!DOCTYPE lolz [<!ENTITY lol \"lol\">]><w:p><w:t>needle</w:t></w:p>",
        )],
        CompressionMethod::Deflated,
    );
    write(&tree, "locked.docx", &encrypted);
    write(&tree, "corrupt.docx", b"PK\x03\x04broken");
    write(&tree, "laughs.docx", &doctype);

    for name in ["locked.docx", "corrupt.docx", "laughs.docx"] {
        assert_eq!(
            doc_text(&tree.path().join(name), "docx", &CancelToken::new()),
            Err(Skip::Document),
            "{name}"
        );
    }

    let normal = search(&tree, "needle docs:on");
    assert!(names(&normal).is_empty());
    assert_eq!(normal.last().unwrap().progress.skipped, 3);

    let inverted = search(&tree, "needle docs:on not:on");
    assert!(names(&inverted).is_empty());
    assert_eq!(inverted.last().unwrap().progress.skipped, 3);
}
