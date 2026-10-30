use crate::{GitError, GitPath, GitRef, ObjectId, RefKind, TreeEntry, TreeKind};

pub(super) fn missing_revision(stderr: &str) -> bool {
    stderr.contains("Needed a single revision")
        || stderr.contains("unknown revision")
        || stderr.contains("ambiguous argument")
}

pub(super) fn parse_oid(output: &[u8]) -> Result<ObjectId, GitError> {
    let token = output
        .split(|byte| byte.is_ascii_whitespace())
        .find(|token| !token.is_empty())
        .ok_or_else(protocol)?;
    let bytes = decode_hex(token)?;
    ObjectId::from_bytes(&bytes).ok_or_else(protocol)
}

fn decode_hex(text: &[u8]) -> Result<Vec<u8>, GitError> {
    if !text.len().is_multiple_of(2) {
        return Err(protocol());
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    let mut index = 0;
    while index < text.len() {
        let high = hex_value(text[index]).ok_or_else(protocol)?;
        let low = hex_value(text[index + 1]).ok_or_else(protocol)?;
        bytes.push((high << 4) | low);
        index += 2;
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) fn parse_refs(output: &[u8]) -> Result<Vec<GitRef>, GitError> {
    let mut refs = Vec::new();
    for line in output.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split(|byte| *byte == 0);
        let name = fields.next().ok_or_else(protocol)?;
        let target = fields.next().ok_or_else(protocol)?;
        if fields.next().is_some() {
            return Err(protocol());
        }
        let name = String::from_utf8(name.to_vec()).map_err(|_| protocol())?;
        let target = parse_oid(target)?;
        let kind = ref_kind(&name);
        refs.push(GitRef { name, kind, target });
    }
    Ok(refs)
}

fn ref_kind(name: &str) -> RefKind {
    if name.starts_with("refs/heads/") {
        RefKind::Branch
    } else if name.starts_with("refs/tags/") {
        RefKind::Tag
    } else if name.starts_with("refs/remotes/") {
        RefKind::Remote
    } else if name == "HEAD" {
        RefKind::Head
    } else {
        RefKind::Other
    }
}

pub(super) fn parse_tree(output: &[u8]) -> Result<Vec<TreeEntry>, GitError> {
    let mut entries = Vec::new();
    for record in output.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        entries.push(parse_tree_record(record)?);
    }
    Ok(entries)
}

fn parse_tree_record(record: &[u8]) -> Result<TreeEntry, GitError> {
    let tab = record
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or_else(protocol)?;
    let header = std::str::from_utf8(&record[..tab]).map_err(|_| protocol())?;
    let mut fields = header.split_ascii_whitespace();
    let mode = fields.next().ok_or_else(protocol)?;
    let _kind = fields.next().ok_or_else(protocol)?;
    let oid = fields.next().ok_or_else(protocol)?;
    let size = fields.next().ok_or_else(protocol)?;
    if fields.next().is_some() {
        return Err(protocol());
    }
    let kind = tree_kind(mode.as_bytes());
    let size = if size == "-" {
        None
    } else {
        Some(size.parse::<u64>().map_err(|_| protocol())?)
    };
    Ok(TreeEntry {
        path: GitPath::new(record[tab + 1..].to_vec()),
        id: parse_oid(oid.as_bytes())?,
        kind,
        size,
    })
}

fn tree_kind(mode: &[u8]) -> TreeKind {
    match mode {
        b"120000" => TreeKind::Link,
        b"160000" => TreeKind::Submodule,
        b"040000" => TreeKind::Folder,
        _ => TreeKind::File,
    }
}

pub(super) fn protocol() -> GitError {
    GitError::Protocol {
        detail: "malformed git output".to_owned(),
    }
}
