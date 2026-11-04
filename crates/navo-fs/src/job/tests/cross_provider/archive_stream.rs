use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::Arc;

use navo_core::{OpKind, OperationPlan, Step, Totals};
use zip::write::SimpleFileOptions;

use crate::{JobOutcome, LocalFs, Sources, run_job_parallel_with_sources, run_job_with_sources};

use super::super::Harness;
use super::local;

const SIZE: usize = 300 * 1024;

fn pattern() -> Vec<u8> {
    (0..SIZE).map(|at| (at % 251) as u8).collect()
}

fn write_zip(path: &Path, inner: &str, bytes: &[u8]) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(inner, SimpleFileOptions::default())
        .unwrap();
    writer.write_all(bytes).unwrap();
    std::fs::write(path, writer.finish().unwrap().into_inner()).unwrap();
}

#[test]
fn a_copy_from_a_zip_writes_equal_bytes_and_no_temporary() {
    let bytes = pattern();
    for workers in [0, 4] {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.zip");
        write_zip(&zip_path, "docs/data.bin", &bytes);
        let source = crate::archive::location(&zip_path, "docs/data.bin").unwrap();
        let target_path = dir.path().join("out.bin");
        let plan = OperationPlan {
            kind: OpKind::Copy,
            steps: vec![Step::CopyFile {
                source,
                target: local(&target_path),
                size: bytes.len() as u64,
            }],
            totals: Totals {
                files: 1,
                folders: 0,
                bytes: bytes.len() as u64,
            },
            conflicts: Vec::new(),
            risks: Vec::new(),
        };
        let sources = Sources::new().with(Arc::new(crate::archive::ArchiveFs::new()));
        let mut harness = Harness::default();
        let outcome = harness.with(&LocalFs::new(), |ctx| match workers {
            0 => run_job_with_sources(&plan, &[], ctx, &sources),
            n => run_job_parallel_with_sources((&plan, &sources), &[], ctx, n),
        });
        assert_eq!(outcome, JobOutcome::Done);
        assert_eq!(std::fs::read(&target_path).unwrap(), bytes);
        assert_eq!(harness.log.placed.len(), 1);
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["a.zip", "out.bin"]);
    }
}
