use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(super) async fn read_paths(paths: Vec<PathBuf>) -> Result<String, String> {
    read_first(&paths)
}

pub(super) fn read_first(paths: &[PathBuf]) -> Result<String, String> {
    let path = paths.first().ok_or("no file was selected")?;
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("the name list exceeds 16 MiB".to_owned());
    }
    let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_bom_and_crlf_names_are_preserved() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "\u{feff}a.txt\r\nb.txt\r\n").unwrap();
        assert_eq!(
            read_first(&[file.path().to_owned()]).unwrap(),
            "a.txt\r\nb.txt\r\n"
        );
    }

    #[test]
    fn invalid_utf8_and_oversized_files_are_rejected() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [0xff]).unwrap();
        assert!(read_first(&[file.path().to_owned()]).is_err());
        file.as_file().set_len(MAX_BYTES + 1).unwrap();
        assert!(
            read_first(&[file.path().to_owned()])
                .unwrap_err()
                .contains("16 MiB")
        );
    }
}
