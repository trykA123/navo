use super::{DriveKind, Identity, PathKind, RedactedPath, Redactor, Salt};

mod canary;
mod paths;
mod props;
mod text;

const SALT_A: [u8; 16] = [7; 16];
const SALT_B: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

fn identity() -> Identity {
    Identity {
        user: Some("admin".to_owned()),
        computer: Some("WORKPC-17".to_owned()),
        domain: Some("CORP".to_owned()),
        profile: Some(r"C:\Users\admin".to_owned()),
    }
}

fn redactor() -> Redactor {
    Redactor::new(Salt::from_bytes(SALT_A), identity())
}

fn file(r: &Redactor, path: &str) -> RedactedPath {
    r.path(path, PathKind::File, DriveKind::Local)
}

fn folder(r: &Redactor, path: &str) -> RedactedPath {
    r.path(path, PathKind::Folder, DriveKind::Local)
}

fn token_end(s: &str) -> Option<usize> {
    let inner = s.strip_prefix('<')?;
    let close = inner.find('>')?;
    let body = &inner[..close];
    let ok = match body.split_once(':') {
        Some(("d" | "f", hex)) => {
            hex.len() == 4 && hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
        }
        Some(("server" | "share", n)) => !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()),
        Some(_) => false,
        None => matches!(body, "user" | "computer" | "domain"),
    };
    ok.then_some(close + 2)
}

fn shape(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match token_end(rest) {
            Some(end) if rest.starts_with("<d:") => {
                out.push_str("<d>");
                rest = &rest[end..];
            }
            Some(end) if rest.starts_with("<f:") => {
                out.push_str("<f>");
                rest = &rest[end..];
            }
            _ => {
                out.push('<');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_tokens(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match token_end(rest) {
            Some(end) => {
                out.push('\u{1}');
                rest = &rest[end..];
            }
            None => {
                out.push('<');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn segment_tokens(s: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        rest = &rest[i..];
        match token_end(rest) {
            Some(end) if rest.starts_with("<d:") || rest.starts_with("<f:") => {
                found.push(rest[..end].to_owned());
                rest = &rest[end..];
            }
            _ => rest = &rest[1..],
        }
    }
    found
}

#[track_caller]
fn assert_absent(output: &str, needles: &[&str]) {
    let lower = output.to_lowercase();
    for needle in needles {
        assert!(
            !lower.contains(&needle.to_lowercase()),
            "{needle:?} leaked into {output:?}"
        );
    }
}

#[test]
fn salt_debug_hides_the_bytes() {
    let salt = Salt::from_bytes([0xAB; 16]);
    assert_eq!(format!("{salt:?}"), "Salt(..)");
}

#[test]
fn redactor_is_shared_across_threads_with_stable_server_numbers() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Redactor>();
    let r = std::sync::Arc::new(redactor());
    let first = file(&r, r"\\nas\photos\a.jpg");
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let r = std::sync::Arc::clone(&r);
            std::thread::spawn(move || file(&r, r"\\NAS\photos\a.jpg"))
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().ok(), Some(first.clone()));
    }
}
