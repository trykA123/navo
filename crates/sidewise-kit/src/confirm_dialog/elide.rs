const ELLIPSIS: char = '…';
const HEAD_SEGMENTS: usize = 2;

pub(super) fn middle(path: &str, max: usize) -> String {
    if path.chars().count() <= max {
        return path.to_owned();
    }
    let separator = if path.contains('\\') { '\\' } else { '/' };
    let segments: Vec<&str> = path.split(separator).collect();
    if segments.len() <= HEAD_SEGMENTS + 1 {
        return cut(path, max);
    }
    let head = segments[..HEAD_SEGMENTS].join(&separator.to_string());
    let rest = &segments[HEAD_SEGMENTS..];
    let room = max.saturating_sub(head.chars().count() + 3);
    let last = rest[rest.len() - 1];
    let mut used = last.chars().count();
    let mut tail = vec![last.to_owned()];
    for segment in rest[..rest.len() - 1].iter().rev() {
        let next = used + 1 + segment.chars().count();
        if next > room {
            break;
        }
        used = next;
        tail.insert(0, (*segment).to_owned());
    }
    if tail.len() == rest.len() {
        return cut(path, max);
    }
    if tail.len() == 1 {
        tail[0] = cut(last, room.max(1));
    }
    let joined = tail.join(&separator.to_string());
    format!("{head}{separator}{ELLIPSIS}{separator}{joined}")
}

fn cut(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let keep = max.saturating_sub(1);
    let front = keep.div_ceil(2);
    let back = keep - front;
    let start: String = text.chars().take(front).collect();
    let end: String = text.chars().skip(count - back).collect();
    format!("{start}{ELLIPSIS}{end}")
}

#[cfg(test)]
mod tests {
    use super::middle;

    #[test]
    fn a_short_path_is_unchanged() {
        assert_eq!(middle(r"C:\work\a.txt", 40), r"C:\work\a.txt");
    }

    #[test]
    fn a_long_windows_path_keeps_the_start_the_last_folders_and_the_file() {
        let path = r"C:\Users\admin\Documents\clients\project\src\main.rs";
        assert_eq!(middle(path, 36), r"C:\Users\…\project\src\main.rs");
    }

    #[test]
    fn a_long_unix_path_keeps_the_root_start() {
        let shown = middle("/home/admin/work/clients/project/src/main.rs", 30);
        assert!(shown.starts_with("/home/…/"), "{shown}");
        assert!(shown.ends_with("/src/main.rs"), "{shown}");
        assert!(shown.chars().count() <= 30);
    }

    #[test]
    fn the_file_name_stays_whole_when_it_fits() {
        let shown = middle(r"D:\one\two\three\four\five\a-long-file-name.txt", 32);
        assert!(shown.ends_with(r"\a-long-file-name.txt"), "{shown}");
        assert!(shown.chars().count() <= 32);
    }

    #[test]
    fn a_name_longer_than_the_room_is_cut_in_its_middle() {
        let name = "x".repeat(60);
        let shown = middle(&format!(r"D:\one\two\{name}"), 30);
        assert!(shown.chars().count() <= 30, "{shown}");
        assert!(shown.starts_with(r"D:\one\…\"), "{shown}");
    }

    #[test]
    fn multibyte_text_is_never_split_inside_a_character() {
        let path = "/données/été/très/long/chemin/vers/le/dossier/fichier-ñandú.txt";
        for max in 30..path.chars().count() {
            let shown = middle(path, max);
            assert!(shown.chars().count() <= max, "{max}: {shown}");
        }
    }
}
