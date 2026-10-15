use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, config::DbConfig};

use super::{CacheError, CacheOpen};

pub(super) fn open(path: &Path) -> Result<(Connection, CacheOpen), CacheError> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(
                io::Error::new(io::ErrorKind::InvalidInput, "cache path is not a file").into(),
            );
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((Connection::open(path)?, CacheOpen::Created));
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    if let Ok(conn) = open_checked(path) {
        return Ok((conn, CacheOpen::Opened));
    }
    let moved_to = quarantine(path)?;
    Ok((Connection::open(path)?, CacheOpen::Recreated { moved_to }))
}

fn open_checked(path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
    check_integrity(&conn)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, false)?;
    Ok(conn)
}

fn check_integrity(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut statement = conn.prepare("PRAGMA quick_check")?;
    let messages = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    match messages.as_slice() {
        [message] if message == "ok" => Ok(()),
        _ => Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
            Some(messages.join("\n")),
        )),
    }
}

fn quarantine(path: &Path) -> Result<PathBuf, io::Error> {
    let files = [
        path.to_path_buf(),
        append(path, "-wal"),
        append(path, "-shm"),
    ];
    let suffix = quarantine_suffix(&files)?;
    for file in &files {
        if exists(file)? {
            fs::rename(file, append(file, &suffix))?;
        }
    }
    Ok(append(path, &suffix))
}

fn quarantine_suffix(files: &[PathBuf]) -> Result<String, io::Error> {
    for number in 1..u64::MAX {
        let suffix = format!(".corrupt-{number}");
        let mut occupied = false;
        for file in files {
            occupied |= exists(&append(file, &suffix))?;
        }
        if !occupied {
            return Ok(suffix);
        }
    }
    Err(io::Error::other("no free corrupt cache filename"))
}

fn exists(path: &Path) -> Result<bool, io::Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn append(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}
