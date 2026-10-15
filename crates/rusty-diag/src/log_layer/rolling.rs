use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

pub(super) struct RollingFile {
    dir: PathBuf,
    stem: String,
    max_bytes: u64,
    files: usize,
    size: u64,
    file: Option<File>,
}

impl RollingFile {
    pub(super) fn open(
        dir: PathBuf,
        stem: String,
        max_bytes: u64,
        files: usize,
    ) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut rolling = Self {
            dir,
            stem,
            max_bytes,
            files: files.max(1),
            size: 0,
            file: None,
        };
        rolling.reopen()?;
        Ok(rolling)
    }

    pub(super) fn write_line(&mut self, line: &str) {
        let bytes = u64::try_from(line.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        if self.size > 0 && self.size.saturating_add(bytes) > self.max_bytes {
            self.rotate();
        }
        if self.file.is_none() && self.reopen().is_err() {
            return;
        }
        let mut buffer = String::with_capacity(line.len() + 1);
        buffer.push_str(line);
        buffer.push('\n');
        if let Some(file) = self.file.as_mut()
            && file.write_all(buffer.as_bytes()).is_ok()
        {
            self.size = self.size.saturating_add(bytes);
        }
    }

    fn reopen(&mut self) -> io::Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(0))?;
        self.size = file.metadata()?.len();
        self.file = Some(file);
        Ok(())
    }

    fn rotate(&mut self) {
        self.file = None;
        self.size = 0;
        let last = self.files - 1;
        let _ = fs::remove_file(self.path(last));
        for index in (0..last).rev() {
            let _ = fs::rename(self.path(index), self.path(index + 1));
        }
    }

    fn path(&self, index: usize) -> PathBuf {
        let name = if index == 0 {
            format!("{}.log", self.stem)
        } else {
            format!("{}.{index}.log", self.stem)
        };
        self.dir.join(name)
    }
}
