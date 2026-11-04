use std::{fmt, io::Write, panic::PanicHookInfo, sync::Mutex};

use serde::{Serialize, Serializer};

use super::{Output, Report, SafeWriter};

const SOURCE_FILES: &[&str] = &[
    "main.rs",
    "lib.rs",
    "guard.rs",
    "fixture.rs",
    "privacy/mod.rs",
    "privacy/error.rs",
    "privacy/extension.rs",
    "privacy/scan.rs",
    "privacy/crash.rs",
    "privacy/tests.rs",
    "privacy/scan_tests.rs",
    "privacy/api_tests.rs",
];

#[derive(Clone, Copy, Debug)]
struct SourceFile(&'static str);

impl SourceFile {
    const UNKNOWN: Self = Self("unknown");

    fn label(self) -> &'static str {
        self.0
    }

    fn from_file(file: &str) -> Self {
        let Some((_, relative)) = file.rsplit_once("crates/navo-bench/src/") else {
            return Self::UNKNOWN;
        };
        SOURCE_FILES
            .iter()
            .copied()
            .find(|source| *source == relative)
            .map(Self)
            .unwrap_or(Self::UNKNOWN)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SourceLocation {
    file: SourceFile,
    line: u32,
}

impl SourceLocation {
    fn from_panic(info: &PanicHookInfo<'_>) -> Self {
        match info.location() {
            Some(location) => {
                let file = SourceFile::from_file(&location.file().replace('\\', "/"));
                let line = if file.label() == SourceFile::UNKNOWN.label() {
                    0
                } else {
                    location.line()
                };
                Self { file, line }
            }
            None => Self {
                file: SourceFile::UNKNOWN,
                line: 0,
            },
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file.label(), self.line)
    }
}

impl Serialize for SourceLocation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

pub fn install_panic_hook<W: Write + Send + 'static>(writer: SafeWriter<W>) {
    let writer = Mutex::new(writer);
    std::panic::set_hook(Box::new(move |info| {
        let report = Report::Panic {
            source: SourceLocation::from_panic(info),
        };
        let mut writer = match writer.lock() {
            Ok(writer) => writer,
            Err(poisoned) => poisoned.into_inner(),
        };
        // A failed crash sink must never fall back to the default payload printer.
        let _ = writer.write(Output::Crash, &report);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_paths_are_replaced_by_allowlisted_labels() {
        let source = SourceFile::from_file("/private/build/crates/navo-bench/src/main.rs");
        assert_eq!(source.label(), "main.rs");
        for file in [
            "/private/secret.rs",
            "crates/navo-bench/src/owner-folder.rs",
            "other/src/main.rs",
        ] {
            assert_eq!(
                SourceFile::from_file(file).label(),
                SourceFile::UNKNOWN.label()
            );
        }
    }
}
