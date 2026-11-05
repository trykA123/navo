use std::time::Instant;

use super::{Term, search};
use crate::table::{Flags, NO_PARENT, NameTable};

fn fixture() -> NameTable {
    const FOLDERS: [&str; 8] = [
        "src", "docs", "build", "assets", "tests", "media", "notes", "tools",
    ];
    const FILES: [&str; 16] = [
        "document", "report", "main", "readme", "invoice", "photo", "video", "song", "config",
        "notes", "backup", "draft", "summary", "plan", "budget", "letter",
    ];
    const EXTS: [&str; 8] = ["txt", "pdf", "rs", "md", "jpg", "mp4", "toml", "docx"];
    let pool: Vec<_> = (0..5000)
        .map(|i| format!("{}-{i:04}.{}", FILES[i % 16], EXTS[i % 8]))
        .collect();
    let mut table = NameTable::new();
    table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut rng = 7_u64;
    for n in 1..2_000_000_usize {
        if n % 10 == 0 {
            let parent = if n <= 10_000 {
                0
            } else {
                ((n / 10) % 1000 + 1) * 10
            };
            table
                .push(
                    u32::try_from(parent).unwrap(),
                    &format!("{}-{n:07}", FOLDERS[(n / 10) % 8]),
                    Flags::FOLDER,
                )
                .unwrap();
        } else {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let name = if rng % 10 < 3 {
                pool[usize::try_from(rng % 5000).unwrap()].clone()
            } else {
                format!("{}-{n:07}.{}", FILES[n % 16], EXTS[n % 8])
            };
            table
                .push(u32::try_from(n - n % 10).unwrap(), &name, Flags::NONE)
                .unwrap();
        }
    }
    table
}

#[test]
#[expect(
    clippy::print_stdout,
    reason = "benchmark emits machine-readable timing measurements"
)]
#[ignore = "timing: two million entries, four Rayon workers, quiet host"]
fn search_scale_timing() {
    let table = fixture();
    let pdf = vec!["pdf".to_owned()];
    let queries = [
        ("doc", vec![Term::Name("doc")]),
        ("report-pdf", vec![Term::Name("report"), Term::Ext(&pdf)]),
        ("src-main", vec![Term::Path("src"), Term::Name("main")]),
    ];
    for (label, terms) in queries {
        let expected = search(&table, &terms, 500, &|| false);
        assert_eq!(expected.len(), 500, "{label}");
        assert_eq!(search(&table, &terms, 500, &|| false), expected);
        let mut times = Vec::new();
        for _ in 0..15 {
            let start = Instant::now();
            let found = search(&table, &terms, 500, &|| false);
            times.push(start.elapsed().as_micros());
            assert_eq!(found, expected);
            for hit in &found {
                let path = table.path(hit.id).unwrap();
                assert!(label != "report-pdf" || path.ends_with(".pdf"));
                assert!(label != "src-main" || path.contains("src"));
            }
        }
        times.sort_unstable();
        println!(
            "SEARCH {label}: median {} min {} max {}",
            times[7], times[0], times[14]
        );
    }
}
