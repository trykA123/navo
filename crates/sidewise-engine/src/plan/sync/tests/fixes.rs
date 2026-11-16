use super::super::{Fences, build_capped, count, parts, sort_rows};
use super::*;

const MODES: [SyncMode; 3] = [
    SyncMode::TwoWay,
    SyncMode::MirrorRight,
    SyncMode::MirrorLeft,
];

fn roots(left: &str, right: &str) -> Sides {
    Sides {
        left: side(left),
        right: side(right),
    }
}

fn plan_between(sides: &Sides, mode: SyncMode) -> Result<SyncPlan, SyncBlock> {
    run_on(
        &[left_only(1, file("a.txt", 1, T0))],
        sides,
        rules(mode, Deletes::Off),
        &[],
    )
}

#[test]
fn nested_or_equal_roots_refuse_the_plan_in_every_mode() {
    let pairs = [
        ("/data", "/data/backup"),
        ("/data/backup", "/data"),
        ("/data", "/data"),
        ("/data/", "/data/backup/deep"),
        ("/data/backup/", "/data/"),
    ];
    for (left, right) in pairs {
        for mode in MODES {
            assert_eq!(
                plan_between(&roots(left, right), mode),
                Err(SyncBlock::SameOrNestedRoots),
                "{left} vs {right} in {mode:?}"
            );
        }
    }
}

#[test]
fn roots_that_differ_only_by_case_refuse_on_an_insensitive_side() {
    let mut windows = roots(r"C:\Data", r"c:\data\backup");
    windows.right.case = CaseRule::Insensitive;
    let mut unix = roots("/data/backup", "/Data");
    unix.left.case = CaseRule::Insensitive;
    let mut equal = roots("/Data", "/data");
    equal.right.case = CaseRule::Insensitive;
    for sides in [windows, unix, equal] {
        for mode in MODES {
            assert_eq!(
                plan_between(&sides, mode),
                Err(SyncBlock::SameOrNestedRoots)
            );
        }
    }
    let sensitive = roots("/Data", "/data/backup");
    assert!(plan_between(&sensitive, SyncMode::TwoWay).is_ok());
}

#[test]
fn spelled_differently_the_same_roots_still_refuse() {
    let pairs = [
        (r"C:\Data", "C:/Data/backup"),
        (r"\\?\C:\Data", r"C:\Data\x"),
        (r"C:\Data\x", r"\\?\C:\Data"),
        (r"\\?\UNC\srv\share", r"\\srv\share\dir"),
        (r"\\srv\share\dir", r"\\?\UNC\srv\share"),
        (r"C:\\Data\\", r"C:\Data\x"),
        ("//data", "/data/x"),
        ("/data/x", "//data"),
        ("/data//a", "/data/a/b"),
    ];
    for (left, right) in pairs {
        for mode in MODES {
            assert_eq!(
                plan_between(&roots(left, right), mode),
                Err(SyncBlock::SameOrNestedRoots),
                "{left} vs {right} in {mode:?}"
            );
        }
    }
}

#[test]
fn roots_with_dot_segments_refuse() {
    let pairs = [
        ("/data/../data", "/data"),
        ("/other", "/data/./x"),
        (r"C:\Data\..\Data", r"C:\Data\x"),
        ("/a/b", "/a/c/.."),
    ];
    for (left, right) in pairs {
        for mode in MODES {
            assert_eq!(
                plan_between(&roots(left, right), mode),
                Err(SyncBlock::RootNotPlain),
                "{left} vs {right} in {mode:?}"
            );
        }
    }
}

#[test]
fn sibling_roots_plan() {
    for (left, right) in [("/data", "/data2"), ("/data2", "/data"), ("/a/x", "/a/y")] {
        for mode in MODES {
            assert!(
                plan_between(&roots(left, right), mode).is_ok(),
                "{left} {right}"
            );
        }
    }
}

fn paired_docs(extra: Vec<Row>) -> Vec<Row> {
    let mut rows = vec![row(
        1,
        Some(folder("Docs")),
        Some(folder("docs")),
        Status::Same,
    )];
    rows.extend(extra);
    rows
}

#[test]
fn a_new_file_goes_under_the_target_spelling_of_its_folder() {
    let rows = paired_docs(vec![right_only(2, file("docs/n.txt", 4, T0))]);
    let copy = run(&rows, TWO_WAY, &[]).expect("plan").copy.expect("copy");
    assert_eq!(
        labels(&copy),
        ["cp /home/right/docs/n.txt > /home/left/Docs/n.txt"]
    );
    assert_eq!(copy.totals, totals(1, 0, 4));
}

#[test]
fn a_new_folder_goes_under_the_target_spelling_of_its_parent() {
    let rows = paired_docs(vec![
        right_only(2, folder("docs/sub")),
        right_only(3, file("docs/sub/m.txt", 2, T0)),
    ]);
    let copy = run(&rows, TWO_WAY, &[]).expect("plan").copy.expect("copy");
    assert_eq!(
        labels(&copy),
        [
            "mk /home/left/Docs/sub",
            "cp /home/right/docs/sub/m.txt > /home/left/Docs/sub/m.txt",
        ]
    );
}

#[test]
fn the_target_spelling_applies_left_to_right_too() {
    let rows = paired_docs(vec![left_only(2, file("Docs/n.txt", 4, T0))]);
    let copy = run(&rows, TWO_WAY, &[]).expect("plan").copy.expect("copy");
    assert_eq!(
        labels(&copy),
        ["cp /home/left/Docs/n.txt > /home/right/docs/n.txt"]
    );
}

fn capped(rows: &[Row], cap: usize) -> Result<SyncPlan, SyncBlock> {
    let choices = choices(&[]);
    build_capped(&request(rows, &sides(), TWO_WAY, &choices), cap)
}

#[test]
fn the_step_count_stops_as_soon_as_it_passes_the_limit() {
    assert_eq!(
        capped(&many_files(1_000), 10),
        Err(SyncBlock::TooManySteps { steps: 11 })
    );
}

#[test]
fn shared_folders_count_once_toward_the_limit() {
    let mut rows = vec![left_only(100, folder("new"))];
    rows.extend((0..10).map(|index| left_only(index, file(&format!("new/f{index}"), 1, T0))));
    let plan = capped(&rows, 11).expect("plan");
    assert_eq!(plan.copy.expect("copy").steps.len(), 11);
    assert_eq!(
        capped(&rows, 10),
        Err(SyncBlock::TooManySteps { steps: 11 })
    );
}

#[test]
fn a_cancelled_build_returns_cancelled() {
    let cancel = CancelToken::new();
    cancel.cancel();
    let sides = sides();
    let choices = choices(&[]);
    for rows in [many_files(3_000), Vec::new()] {
        let request = SyncRequest {
            cancel: &cancel,
            ..request(&rows, &sides, TWO_WAY, &choices)
        };
        assert_eq!(build(&request), Err(SyncBlock::Cancelled));
    }
}

fn cancelled_request<'a>(
    token: &'a CancelToken,
    rows: &'a [Row],
    sides: &'a Sides,
    choices: &'a HashMap<u64, SyncAction>,
) -> SyncRequest<'a> {
    SyncRequest {
        cancel: token,
        ..request(rows, sides, TWO_WAY, choices)
    }
}

#[test]
fn every_row_loop_stops_on_a_cancelled_token() {
    let token = CancelToken::new();
    token.cancel();
    let sides = sides();
    let choices = choices(&[]);
    for (count, expect_cancel) in [(3_000, true), (0, false)] {
        let rows = many_files(count);
        let stop = Err(SyncBlock::Cancelled);
        let live = request(&rows, &sides, TWO_WAY, &choices);
        let dead = cancelled_request(&token, &rows, &sides, &choices);
        let fences = Fences::of(&live).expect("fences");
        let sorted = || sort_rows(&live, &fences).expect("sorted");
        let present = count::present_folders(&live);
        let results = [
            Fences::of(&dead).map(|_| ()),
            sort_rows(&dead, &fences).map(|_| ()),
            parts::candidates(&dead, sorted(), &present, MAX_NODES).map(|_| ()),
        ];
        for result in results {
            assert_eq!(result, if expect_cancel { stop.clone() } else { Ok(()) });
        }
    }
}

#[test]
fn unchecked_rows_without_a_time_are_different() {
    let status = Status::NotChecked(Reason::ReadError);
    let timed = || file("a.txt", 5, T0);
    let untimed = || entry("a.txt", EntryKind::File, 5, None);
    let mirror = rules(SyncMode::MirrorRight, Deletes::Off);
    for (left, right) in [
        (untimed(), timed()),
        (timed(), untimed()),
        (untimed(), untimed()),
    ] {
        let item = row(1, Some(left), Some(right), status);
        assert_eq!(recommend(&item, TWO_WAY), Verdict::Conflict);
        assert_eq!(
            recommend(&item, mirror),
            Verdict::Copy(Direction::LeftToRight)
        );
    }
    let close = row(1, Some(timed()), Some(file("a.txt", 5, T0 + 2_000)), status);
    assert_eq!(recommend(&close, TWO_WAY), Verdict::Same);
    assert_eq!(recommend(&close, mirror), Verdict::Same);
}
