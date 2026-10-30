use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::process::{Command, Stdio};
use std::thread;

use super::corpus::Pair;

pub(super) struct History {
    pub pairs: Vec<Pair>,
    pub commits: usize,
    pub skipped: usize,
}

#[derive(Debug)]
struct Record {
    old_sha: String,
    new_sha: String,
    old_blob: bool,
    new_blob: bool,
}

pub(super) fn load(wanted: usize) -> Result<History, String> {
    let revs = first_parent_revs(wanted + 1)?;
    if revs.len() < 2 {
        return Err("history has fewer than two first-parent commits".to_owned());
    }
    let commits = revs.len() - 1;
    let mut pairs = Vec::new();
    let mut skipped = 0;
    for window in revs.windows(2) {
        let (new, old) = (&window[0], &window[1]);
        let raw = git(&["diff-tree", "-r", "-z", "--no-renames", "--raw", old, new])?;
        let records = parse_records(&raw)?;
        let shas: HashSet<String> = records
            .iter()
            .flat_map(|record| {
                [
                    (record.old_blob, &record.old_sha),
                    (record.new_blob, &record.new_sha),
                ]
            })
            .filter(|(blob, sha)| *blob && !is_zero(sha))
            .map(|(_, sha)| sha.clone())
            .collect();
        let blobs = read_blobs(&shas)?;
        for record in records {
            if !record.old_blob && !is_zero(&record.old_sha)
                || !record.new_blob && !is_zero(&record.new_sha)
            {
                skipped += 1;
                continue;
            }
            match text_pair(&record, &blobs) {
                Some(pair) if pair.old != pair.new => pairs.push(pair),
                _ => skipped += 1,
            }
        }
    }
    if pairs.is_empty() {
        return Err("no text files changed in the history corpus".to_owned());
    }
    Ok(History {
        pairs,
        commits,
        skipped,
    })
}

fn first_parent_revs(count: usize) -> Result<Vec<String>, String> {
    let count = format!("--max-count={count}");
    let output = git(&["rev-list", "--first-parent", &count, "HEAD"])?;
    let text = String::from_utf8(output).map_err(|error| error.to_string())?;
    Ok(text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn git(args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|error| format!("starting git: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {stderr}", args.join(" ")));
    }
    Ok(output.stdout)
}

fn parse_records(raw: &[u8]) -> Result<Vec<Record>, String> {
    let mut records = Vec::new();
    let mut chunks = raw.split(|byte| *byte == 0);
    while let Some(header) = chunks.next() {
        if header.is_empty() {
            continue;
        }
        if !header.starts_with(b":") {
            return Err("unexpected diff-tree record".to_owned());
        }
        let path = chunks
            .next()
            .ok_or_else(|| "diff-tree record without a path".to_owned())?;
        let _path = path;
        let fields: Vec<&[u8]> = header[1..].split(|byte| *byte == b' ').collect();
        if fields.len() != 5 {
            return Err("unexpected diff-tree header".to_owned());
        }
        records.push(Record {
            old_sha: String::from_utf8_lossy(fields[2]).into_owned(),
            new_sha: String::from_utf8_lossy(fields[3]).into_owned(),
            old_blob: is_blob(fields[0]),
            new_blob: is_blob(fields[1]),
        });
    }
    Ok(records)
}

fn is_blob(mode: &[u8]) -> bool {
    matches!(mode, b"100644" | b"100755")
}

fn is_zero(sha: &str) -> bool {
    sha.bytes().all(|byte| byte == b'0')
}

fn text_pair(record: &Record, blobs: &HashMap<String, Vec<u8>>) -> Option<Pair> {
    let old = side_bytes(record.old_blob, &record.old_sha, blobs)?;
    let new = side_bytes(record.new_blob, &record.new_sha, blobs)?;
    let old = text(old)?;
    let new = text(new)?;
    Some(Pair { old, new })
}

fn side_bytes<'a>(blob: bool, sha: &str, blobs: &'a HashMap<String, Vec<u8>>) -> Option<&'a [u8]> {
    if !blob || is_zero(sha) {
        return Some(&[]);
    }
    blobs.get(sha).map(Vec::as_slice)
}

fn text(bytes: &[u8]) -> Option<String> {
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes.to_vec()).ok()
}

fn read_blobs(shas: &HashSet<String>) -> Result<HashMap<String, Vec<u8>>, String> {
    if shas.is_empty() {
        return Ok(HashMap::new());
    }
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("starting git cat-file: {error}"))?;
    let stdin = child.stdin.take().ok_or("git cat-file has no stdin")?;
    let requests: Vec<String> = shas.iter().cloned().collect();
    let count = requests.len();
    let writer = thread::spawn(move || -> Result<(), String> {
        let mut writer = BufWriter::new(stdin);
        for sha in requests {
            writeln!(writer, "{sha}").map_err(|error| error.to_string())?;
        }
        writer.flush().map_err(|error| error.to_string())
    });
    let stdout = child.stdout.take().ok_or("git cat-file has no stdout")?;
    let blobs = read_batch(BufReader::new(stdout), count)?;
    let status = child.wait().map_err(|error| error.to_string())?;
    writer
        .join()
        .map_err(|_| "cat-file writer panicked".to_owned())??;
    if !status.success() {
        return Err(format!("git cat-file failed: {status}"));
    }
    Ok(blobs)
}

fn read_batch(
    mut reader: BufReader<std::process::ChildStdout>,
    count: usize,
) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut blobs = HashMap::new();
    for _ in 0..count {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .map_err(|error| error.to_string())?;
        let fields: Vec<&str> = header.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(format!("unexpected cat-file header: {header:?}"));
        }
        if fields[1] != "blob" {
            continue;
        }
        let size = fields
            .get(2)
            .ok_or_else(|| format!("cat-file header without a size: {header:?}"))?
            .parse::<usize>()
            .map_err(|error| format!("bad cat-file size: {error}"))?;
        let mut bytes = vec![0; size];
        reader
            .read_exact(&mut bytes)
            .map_err(|error| error.to_string())?;
        let mut newline = [0];
        reader
            .read_exact(&mut newline)
            .map_err(|error| error.to_string())?;
        blobs.insert(fields[0].to_owned(), bytes);
    }
    Ok(blobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_raw_record_with_path() {
        let raw = b":100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb M\0src/main.rs\0";
        let records = parse_records(raw).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].old_blob && records[0].new_blob);
    }

    #[test]
    fn rejects_patch_text_as_a_record() {
        assert!(parse_records(b"diff --git a b").is_err());
    }
}
