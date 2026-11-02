const CRLF: &str = "\r\n";

// Above this many cells the line match falls back to the common head and tail only.
const MAX_CELLS: usize = 1 << 22;

pub(super) fn restore(source: &str, edited: &str) -> String {
    let crlf = source.matches(CRLF).count();
    if crlf == 0 {
        edited.to_owned()
    } else if crlf == source.matches('\n').count() {
        edited.replace('\n', CRLF)
    } else {
        mixed(source, edited)
    }
}

fn mixed(source: &str, edited: &str) -> String {
    let old: Vec<(&str, &str)> = source.split_inclusive('\n').map(split_ending).collect();
    let new: Vec<(&str, &str)> = edited.split_inclusive('\n').map(split_ending).collect();
    let fresh = old.first().map_or("\n", |(_, ending)| *ending);
    let pairs = align(&bodies(&old), &bodies(&new));
    let mut out = String::with_capacity(edited.len() + old.len());
    for ((body, ending), from) in new.iter().zip(pairs) {
        out.push_str(body);
        if !ending.is_empty() {
            let kept = from.and_then(|at| old.get(at)).map(|(_, e)| *e);
            out.push_str(kept.filter(|e| !e.is_empty()).unwrap_or(fresh));
        }
    }
    out
}

fn bodies<'a>(lines: &[(&'a str, &str)]) -> Vec<&'a str> {
    lines.iter().map(|(body, _)| *body).collect()
}

fn split_ending(line: &str) -> (&str, &str) {
    if let Some(body) = line.strip_suffix(CRLF) {
        (body, CRLF)
    } else if let Some(body) = line.strip_suffix('\n') {
        (body, "\n")
    } else {
        (line, "")
    }
}

fn align(old: &[&str], new: &[&str]) -> Vec<Option<usize>> {
    let mut anchors = common(old, new);
    anchors.push((old.len(), new.len()));
    let mut found = vec![None; new.len()];
    let mut start = (0, 0);
    for (o, n) in anchors {
        for (at, from) in (start.1..n).zip(start.0..o) {
            found[at] = Some(from);
        }
        if n < new.len() {
            found[n] = Some(o);
        }
        start = (o + 1, n + 1);
    }
    found
}

fn common(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let (old_rest, new_rest) = (&old[head..], &new[head..]);
    let tail = old_rest
        .iter()
        .rev()
        .zip(new_rest.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let middle = (
        &old_rest[..old_rest.len() - tail],
        &new_rest[..new_rest.len() - tail],
    );
    let mut pairs: Vec<(usize, usize)> = (0..head).map(|at| (at, at)).collect();
    pairs.extend(
        longest_common(middle.0, middle.1)
            .into_iter()
            .map(|(o, n)| (head + o, head + n)),
    );
    let (old_tail, new_tail) = (old.len() - tail, new.len() - tail);
    pairs.extend((0..tail).map(|at| (old_tail + at, new_tail + at)));
    pairs
}

fn longest_common(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let width = new.len() + 1;
    if (old.len() + 1).saturating_mul(width) > MAX_CELLS {
        return Vec::new();
    }
    let mut table = vec![0_u32; (old.len() + 1) * width];
    for o in (0..old.len()).rev() {
        for n in (0..new.len()).rev() {
            table[o * width + n] = if old[o] == new[n] {
                table[(o + 1) * width + n + 1] + 1
            } else {
                table[(o + 1) * width + n].max(table[o * width + n + 1])
            };
        }
    }
    let (mut o, mut n, mut pairs) = (0, 0, Vec::new());
    while o < old.len() && n < new.len() {
        if old[o] == new[n] {
            pairs.push((o, n));
            (o, n) = (o + 1, n + 1);
        } else if table[(o + 1) * width + n] >= table[o * width + n + 1] {
            o += 1;
        } else {
            n += 1;
        }
    }
    pairs
}
