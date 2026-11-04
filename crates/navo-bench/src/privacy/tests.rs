use std::{
    io,
    io::{Read, Seek, SeekFrom, Write},
    sync::{Arc, Mutex},
};

use super::*;
use crate::fixture::{
    self, PRIVATE_CONTENT, PRIVATE_EXTENSION, PRIVATE_FILE, PRIVATE_FOLDER, PRIVATE_QUERY,
};

const OUTPUTS: [Output; 6] = [
    Output::Html,
    Output::Json,
    Output::Console,
    Output::Log,
    Output::Trace,
    Output::Crash,
];

#[test]
fn medium_planted_fixture_stays_private_through_every_output() {
    let fixture = fixture::set().unwrap();
    let owner = fixture.root().to_string_lossy();
    let path = fixture.privacy_file.to_string_lossy();
    let content = std::fs::read_to_string(&fixture.privacy_file).unwrap();
    let planted = [
        &*owner,
        &*path,
        &content,
        PRIVATE_CONTENT,
        PRIVATE_EXTENSION,
        PRIVATE_FILE,
        PRIVATE_FOLDER,
        PRIVATE_QUERY,
    ];
    let scanner = LeakScanner::new(&planted).unwrap();
    let error = PrivacyError::from_io(
        PathRole::OwnerFolder,
        io::Error::other(format!("{path}: {content}")),
    );
    let reports = fixture_reports(error);
    let mut file = tempfile::tempfile().unwrap();
    for output in OUTPUTS {
        let mut buffer = Vec::new();
        let mut writer = SafeWriter::new(&mut buffer, scanner.clone());
        for report in &reports {
            writer.write(output, report).unwrap();
            SafeWriter::new(&mut file, scanner.clone())
                .write(output, report)
                .unwrap();
        }
        assert!(!buffer.is_empty());
        assert_no_leaks(&buffer, &planted).unwrap();
        assert_no_leaks(&buffer, &[fixture::RARE_EXT]).unwrap();
    }
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_no_leaks(&bytes, &planted).unwrap();
}

fn fixture_reports(error: PrivacyError) -> [Report; 8] {
    [
        Report::Measurement {
            label: Label::Files.into(),
            value: 1,
        },
        Report::Extension {
            extension: fold_extension(PRIVATE_EXTENSION, 1, 1),
            files: 1,
            bytes: 42,
        },
        Report::Extension {
            extension: fold_extension(fixture::RARE_EXT, 1, 1),
            files: 1,
            bytes: 0,
        },
        Report::Error { error },
        Report::Message {
            label: safe_text!(PRIVATE_QUERY),
        },
        Report::Message {
            label: safe_text!(PRIVATE_CONTENT),
        },
        Report::Message {
            label: safe_text!(PRIVATE_FILE),
        },
        Report::Message {
            label: safe_text!(PRIVATE_FOLDER),
        },
    ]
}

#[test]
fn unsafe_static_labels_and_allowed_extensions_become_count_only_stubs() {
    for output in OUTPUTS {
        let mut bytes = Vec::new();
        let scanner = LeakScanner::new(&["zqxjv", "ab12"]).unwrap();
        let mut writer = SafeWriter::new(&mut bytes, scanner);
        writer
            .write(
                output,
                &Report::Message {
                    label: safe_text!("zqxjv"),
                },
            )
            .unwrap();
        writer
            .write(
                output,
                &Report::Extension {
                    extension: fold_extension("ab12", 10, 2),
                    files: 10,
                    bytes: 0,
                },
            )
            .unwrap();
        for label in [
            safe_text!(r"\u007a\u0071\u0078\u006a\u0076"),
            safe_text!("%7a%71%78%6a%76"),
        ] {
            writer.write(output, &Report::Message { label }).unwrap();
        }
        assert_eq!(bytes, b"1\n1\n1\n1\n");
    }
}

#[test]
fn even_the_redaction_stub_is_checked_before_writing() {
    let mut bytes = Vec::new();
    let mut writer = SafeWriter::new(&mut bytes, LeakScanner::new(&["zqxjv", "1"]).unwrap());
    let result = writer.write(
        Output::Json,
        &Report::Message {
            label: safe_text!("zqxjv"),
        },
    );
    assert_eq!(result.unwrap_err().code, ErrorCode::Redacted);
    assert!(bytes.is_empty());
}

#[test]
fn html_entities_cannot_hide_confidential_names_from_the_writer() {
    let mut bytes = Vec::new();
    let scanner = LeakScanner::new(&[PRIVATE_FOLDER]).unwrap();
    SafeWriter::new(&mut bytes, scanner)
        .write(
            Output::Html,
            &Report::Message {
                label: safe_text!(PRIVATE_FOLDER),
            },
        )
        .unwrap();
    assert_eq!(bytes, b"1\n");
}

#[test]
fn errors_keep_only_roles_and_kinds_even_when_debugged_or_chained() {
    for role in [PathRole::OwnerFolder, PathRole::ScratchRoot] {
        let error = PrivacyError::from_io(
            role,
            io::Error::new(io::ErrorKind::PermissionDenied, PRIVATE_QUERY),
        );
        assert_eq!(error.to_string(), format!("{role}:io-PermissionDenied"));
        assert_no_leaks(format!("{error:?} {error}").as_bytes(), &[PRIVATE_QUERY]).unwrap();
        assert!(std::error::Error::source(&error).is_none());
    }
}

#[test]
fn diagnostic_debug_output_hides_the_scanner_patterns_and_underlying_sink() {
    let fixture = fixture::set().unwrap();
    let path = fixture.privacy_file.to_string_lossy();
    let planted = [&*path, PRIVATE_QUERY];
    let scanner = LeakScanner::new(&planted).unwrap();
    let file = std::fs::File::open(&fixture.privacy_file).unwrap();
    let writer = SafeWriter::new(file, scanner.clone());
    assert_no_leaks(format!("{scanner:?} {writer:?}").as_bytes(), &planted).unwrap();
}

#[test]
fn logger_defaults_off_and_routes_enabled_events_and_traces_through_the_filter() {
    let mut bytes = Vec::new();
    let report = Report::Message {
        label: safe_text!(PRIVATE_QUERY),
    };
    let mut logger = SafeLogger::new(SafeWriter::new(
        &mut bytes,
        LeakScanner::new(&[PRIVATE_QUERY]).unwrap(),
    ));
    logger.event(&report).unwrap();
    logger.trace(&report).unwrap();
    assert!(logger.writer.sink.is_empty());
    logger.set_level(LogLevel::Events);
    logger.event(&report).unwrap();
    logger.trace(&report).unwrap();
    assert_eq!(logger.writer.sink.as_slice(), b"1\n");
    logger.set_level(LogLevel::Trace);
    logger.event(&report).unwrap();
    logger.trace(&report).unwrap();
    assert_eq!(bytes, b"1\n1\n1\n");
    assert_no_leaks(&bytes, &[PRIVATE_QUERY]).unwrap();
}

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn forced_panic_never_prints_payload_or_build_directory() {
    let fixture = fixture::set().unwrap();
    let path = fixture.privacy_file.to_string_lossy();
    let planted = [
        &*path,
        PRIVATE_QUERY,
        PRIVATE_FILE,
        PRIVATE_FOLDER,
        PRIVATE_CONTENT,
        PRIVATE_EXTENSION,
    ];
    let captured = Capture(Arc::default());
    let writer = SafeWriter::new(captured.clone(), LeakScanner::new(&planted).unwrap());
    let previous = std::panic::take_hook();
    install_panic_hook(writer);
    let result = std::panic::catch_unwind(|| panic!("{path}: {PRIVATE_QUERY}: {PRIVATE_CONTENT}"));
    std::panic::set_hook(previous);
    assert!(result.is_err());
    let bytes = captured.0.lock().unwrap();
    assert_no_leaks(&bytes, &planted).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(!text.contains(env!("CARGO_MANIFEST_DIR")));
    assert!(text.starts_with("navo-bench panic privacy/tests.rs:"));
    assert!(
        text.trim_end()
            .rsplit(':')
            .next()
            .unwrap()
            .parse::<u32>()
            .unwrap()
            > 0
    );
}

struct FaultSink {
    bytes: Vec<u8>,
    remaining: usize,
    fail_flush: bool,
}

impl Write for FaultSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other(PRIVATE_QUERY));
        }
        let length = self.remaining.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..length]);
        self.remaining -= length;
        Ok(length)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.fail_flush {
            Err(io::Error::other(PRIVATE_QUERY))
        } else {
            Ok(())
        }
    }
}

#[test]
fn every_partial_write_and_flush_failure_keeps_output_and_errors_private() {
    let report = Report::Message {
        label: safe_text!(PRIVATE_QUERY),
    };
    for remaining in 0..=2 {
        let mut sink = FaultSink {
            bytes: Vec::new(),
            remaining,
            fail_flush: remaining == 2,
        };
        let mut writer = SafeWriter::new(&mut sink, LeakScanner::new(&[PRIVATE_QUERY]).unwrap());
        let error = writer.write(Output::Json, &report).unwrap_err();
        assert_eq!(error.code, ErrorCode::Io(io::ErrorKind::Other));
        assert_no_leaks(&sink.bytes, &[PRIVATE_QUERY]).unwrap();
        assert_no_leaks(error.to_string().as_bytes(), &[PRIVATE_QUERY]).unwrap();
        assert_eq!(sink.bytes, b"1\n"[..remaining]);
    }
}

#[test]
fn html_text_is_checked_before_and_after_escaping() {
    let scanner = LeakScanner::new(&["zqxjv&private"]).unwrap();
    let mut page = Html::new();
    page.markup("<p>").text("zqxjv&private").markup("</p>");
    let mut bytes = Vec::new();
    SafeWriter::new(&mut bytes, scanner.clone())
        .write_html(&page)
        .unwrap();
    assert_eq!(bytes, b"1\n");
    let mut clean = Html::new();
    clean.markup("<p>").text("a&b").markup("</p>");
    let mut bytes = Vec::new();
    SafeWriter::new(&mut bytes, scanner)
        .write_html(&clean)
        .unwrap();
    assert_eq!(bytes, b"<p>a&amp;b</p>");
}
