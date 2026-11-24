use std::{fs, io::Write, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DataSet {
    pub name: &'static str,
    pub files: u64,
    pub folders: u64,
    pub file_bytes: u64,
}

impl DataSet {
    pub fn bytes(self) -> u64 {
        self.files * self.file_bytes
    }
}

pub(super) const DATA_SETS: [DataSet; 3] = [
    DataSet {
        name: "small",
        files: 10_000,
        folders: 1,
        file_bytes: 4 * 1024,
    },
    DataSet {
        name: "medium",
        files: 200,
        folders: 1,
        file_bytes: 5 * 1024 * 1024,
    },
    DataSet {
        name: "large",
        files: 2,
        folders: 1,
        file_bytes: 1024 * 1024 * 1024,
    },
];

/// The non-default 100,000-file set used to measure per-step copy cost.
pub(super) const STEPS: DataSet = DataSet {
    name: "steps",
    files: 100_000,
    folders: 100,
    file_bytes: 16,
};

/// Every selectable set, including the non-default `steps` set.
pub(super) const ALL_SETS: [DataSet; 4] = [DATA_SETS[0], DATA_SETS[1], DATA_SETS[2], STEPS];

/// Writes a set under `folder`, spread evenly over `set.folders` subfolders.
pub(super) fn generate(folder: &Path, set: DataSet) -> Result<(), String> {
    fs::create_dir(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    if set.folders == 0 || !set.files.is_multiple_of(set.folders) {
        return Err(format!(
            "{} files do not split into {} folders",
            set.files, set.folders
        ));
    }
    let mut state = 0x6a09_e667_f3bc_c909 ^ set.file_bytes;
    let mut buffer = vec![0; 1024 * 1024];
    let per_folder = set.files / set.folders;
    for folder_index in 0..set.folders {
        let folder = if set.folders == 1 {
            folder.to_path_buf()
        } else {
            let path = folder.join(format!("folder-{folder_index:03}"));
            fs::create_dir(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            path
        };
        for index in 0..per_folder {
            let path = folder.join(format!("file-{index:05}.bin"));
            write_file(&path, set.file_bytes, &mut state, &mut buffer)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn write_file(path: &Path, bytes: u64, state: &mut u64, buffer: &mut [u8]) -> std::io::Result<()> {
    let mut file = fs::File::create_new(path)?;
    let mut remaining = bytes;
    while remaining > 0 {
        let length = remaining.min(buffer.len() as u64) as usize;
        fill(state, &mut buffer[..length]);
        file.write_all(&buffer[..length])?;
        remaining -= length as u64;
    }
    file.sync_all()
}

pub(super) fn fill(state: &mut u64, buffer: &mut [u8]) {
    for chunk in buffer.chunks_mut(8) {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = *state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        chunk.copy_from_slice(&value.to_le_bytes()[..chunk.len()]);
    }
}
