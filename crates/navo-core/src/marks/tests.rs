use super::*;

fn text(text: &str) -> Needle {
    Needle::Text {
        text: text.to_owned(),
        match_case: false,
    }
}

fn colours(set: &MarkSet) -> Vec<(Needle, u8)> {
    set.iter()
        .map(|mark| (mark.needle.clone(), mark.colour))
        .collect()
}

fn three() -> MarkSet {
    let mut set = MarkSet::default();
    set.set(text("error"), 0).unwrap();
    set.set(text("warning"), 3).unwrap();
    set.set(Needle::Bytes(vec![0x40, 0x00]), 7).unwrap();
    set
}

fn marked(starts: &[u64]) -> Bookmarks {
    let mut bookmarks = Bookmarks::default();
    for &at in starts {
        bookmarks.toggle(at);
    }
    bookmarks
}

fn history(needles: &[Needle]) -> SearchHistory {
    let mut history = SearchHistory::default();
    for needle in needles {
        history.push(needle.clone());
    }
    history
}

fn listed(history: &SearchHistory) -> Vec<Needle> {
    history.iter().cloned().collect()
}

#[test]
fn mark_set_keeps_marks_in_the_order_set() {
    let set = three();
    let want = vec![
        (text("error"), 0),
        (text("warning"), 3),
        (Needle::Bytes(vec![0x40, 0x00]), 7),
    ];
    assert_eq!(colours(&set), want);
    assert!(!set.is_empty());
}

#[test]
fn mark_set_gives_the_colour_of_a_needle() {
    let set = three();
    assert_eq!(set.colour_of(&text("warning")), Some(3));
    assert_eq!(set.colour_of(&text("missing")), None);
}

#[test]
fn mark_set_recolours_in_place() {
    let mut set = three();
    set.set(text("error"), 5).unwrap();
    let want = vec![
        (text("error"), 5),
        (text("warning"), 3),
        (Needle::Bytes(vec![0x40, 0x00]), 7),
    ];
    assert_eq!(colours(&set), want);
}

#[test]
fn mark_set_removes_a_needle() {
    let mut set = three();
    assert!(set.remove(&text("warning")));
    assert!(!set.remove(&text("warning")));
    let want = vec![(text("error"), 0), (Needle::Bytes(vec![0x40, 0x00]), 7)];
    assert_eq!(colours(&set), want);
}

#[test]
fn mark_set_clear_empties_it() {
    let mut set = three();
    assert_eq!(set.iter().count(), 3);
    set.clear();
    assert!(set.is_empty());
    assert_eq!(set.iter().count(), 0);
}

#[test]
fn mark_set_rejects_colour_eight() {
    let mut set = MarkSet::default();
    assert_eq!(
        set.set(text("error"), MARK_COLOURS),
        Err(MarkError::Colour(8))
    );
    assert!(set.set(text("error"), MARK_COLOURS - 1).is_ok());
    assert_eq!(set.colour_of(&text("error")), Some(7));
}

#[test]
fn mark_set_rejects_a_thirty_third_mark() {
    let mut set = MarkSet::default();
    for index in 0..MAX_MARKS {
        set.set(text(&format!("word{index}")), 0).unwrap();
    }
    assert_eq!(set.set(text("overflow"), 1), Err(MarkError::Full));
    assert_eq!(set.iter().count(), MAX_MARKS);
}

#[test]
fn mark_set_recolours_when_full() {
    let mut set = MarkSet::default();
    for index in 0..MAX_MARKS {
        set.set(text(&format!("word{index}")), 0).unwrap();
    }
    assert_eq!(set.set(text("word5"), 2), Ok(()));
    assert_eq!(set.colour_of(&text("word5")), Some(2));
}

#[test]
fn mark_set_survives_a_postcard_round_trip() {
    let set = three();
    assert!(!set.is_empty());
    let bytes = postcard::to_stdvec(&set).unwrap();
    let back: MarkSet = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(colours(&back), colours(&set));
}

#[test]
fn bookmarks_toggle_on_then_off() {
    let mut bookmarks = Bookmarks::default();
    assert!(bookmarks.toggle(40));
    assert_eq!(bookmarks.iter().collect::<Vec<_>>(), vec![40]);
    assert!(!bookmarks.toggle(40));
    assert!(bookmarks.is_empty());
}

#[test]
fn bookmarks_iterate_in_offset_order() {
    let bookmarks = marked(&[300, 10, 120]);
    assert_eq!(bookmarks.iter().collect::<Vec<_>>(), vec![10, 120, 300]);
    assert!(!bookmarks.is_empty());
}

#[test]
fn bookmarks_next_is_strictly_after() {
    let bookmarks = marked(&[10, 120, 300]);
    assert_eq!(bookmarks.next_after(10), Some(120));
    assert_eq!(bookmarks.next_after(50), Some(120));
    assert_eq!(bookmarks.next_after(0), Some(10));
}

#[test]
fn bookmarks_next_wraps_to_the_first() {
    let bookmarks = marked(&[10, 120, 300]);
    assert_eq!(bookmarks.next_after(300), Some(10));
    assert_eq!(bookmarks.next_after(u64::MAX), Some(10));
}

#[test]
fn bookmarks_previous_is_strictly_before() {
    let bookmarks = marked(&[10, 120, 300]);
    assert_eq!(bookmarks.previous_before(300), Some(120));
    assert_eq!(bookmarks.previous_before(200), Some(120));
    assert_eq!(bookmarks.previous_before(u64::MAX), Some(300));
}

#[test]
fn bookmarks_previous_wraps_to_the_last() {
    let bookmarks = marked(&[10, 120, 300]);
    assert_eq!(bookmarks.previous_before(10), Some(300));
    assert_eq!(bookmarks.previous_before(0), Some(300));
}

#[test]
fn bookmarks_single_wraps_to_itself() {
    let bookmarks = marked(&[64]);
    assert_eq!(bookmarks.next_after(64), Some(64));
    assert_eq!(bookmarks.previous_before(64), Some(64));
}

#[test]
fn bookmarks_empty_has_no_next_or_previous() {
    let mut bookmarks = Bookmarks::default();
    assert!(bookmarks.toggle(64));
    assert!(!bookmarks.toggle(64));
    assert!(bookmarks.is_empty());
    assert_eq!(bookmarks.next_after(0), None);
    assert_eq!(bookmarks.previous_before(100), None);
}

#[test]
fn search_history_lists_newest_first() {
    let got = history(&[text("alpha"), text("beta"), text("gamma")]);
    assert_eq!(
        listed(&got),
        vec![text("gamma"), text("beta"), text("alpha")]
    );
}

#[test]
fn search_history_moves_a_repeat_to_the_front() {
    let got = history(&[text("alpha"), text("beta"), text("alpha")]);
    assert_eq!(listed(&got), vec![text("alpha"), text("beta")]);
}

#[test]
fn search_history_drops_the_oldest_past_the_cap() {
    let needles: Vec<Needle> = (0..SearchHistory::MAX + 5)
        .map(|index| text(&format!("word{index}")))
        .collect();
    let got = listed(&history(&needles));
    assert_eq!(got.len(), SearchHistory::MAX);
    assert_eq!(got.first(), Some(&text("word54")));
    assert_eq!(got.last(), Some(&text("word5")));
}

#[test]
fn search_history_ignores_empty_needles() {
    let empty_text = Needle::Text {
        text: String::new(),
        match_case: true,
    };
    let got = history(&[text("alpha"), empty_text, Needle::Bytes(Vec::new())]);
    assert_eq!(listed(&got), vec![text("alpha")]);
}

#[test]
fn search_history_keeps_text_and_hex_with_the_same_bytes_apart() {
    let hex = Needle::Bytes(b"AB".to_vec());
    let got = history(&[text("AB"), hex.clone()]);
    assert_eq!(listed(&got), vec![hex, text("AB")]);
}

#[test]
fn search_history_keeps_match_case_variants_apart() {
    let exact = Needle::Text {
        text: "alpha".to_owned(),
        match_case: true,
    };
    let got = history(&[text("alpha"), exact.clone()]);
    assert_eq!(listed(&got), vec![exact, text("alpha")]);
}

#[test]
fn ticks_map_offsets_to_track_rows() {
    let got = ticks(&[(0, 1), (500, 2), (999, 3)], 1000, 10);
    assert_eq!(got, vec![(0, 1), (5, 2), (9, 3)]);
}

#[test]
fn ticks_clamp_to_the_last_row() {
    let got = ticks(&[(1000, 4), (5000, 5)], 1000, 10);
    assert_eq!(got, vec![(9, 4), (9, 5)]);
}

#[test]
fn ticks_do_not_overflow_on_huge_offsets() {
    let got = ticks(&[(u64::MAX - 1, 6)], u64::MAX, 100);
    assert_eq!(got, vec![(99, 6)]);
}

#[test]
fn ticks_keep_one_per_row_and_colour() {
    let got = ticks(&[(10, 1), (20, 1), (30, 2), (15, 1)], 1000, 10);
    assert_eq!(got, vec![(0, 1), (0, 2)]);
}

#[test]
fn ticks_sort_by_row_then_colour() {
    let got = ticks(&[(900, 0), (100, 7), (100, 2), (500, 4)], 1000, 10);
    assert_eq!(got, vec![(1, 2), (1, 7), (5, 4), (9, 0)]);
}

#[test]
fn ticks_of_an_empty_file_or_track_are_empty() {
    let spans = [(0, 1), (50, 2)];
    assert_eq!(ticks(&spans, 100, 4), vec![(0, 1), (2, 2)]);
    assert!(ticks(&spans, 0, 4).is_empty());
    assert!(ticks(&spans, 100, 0).is_empty());
}

#[test]
fn mark_set_rejects_decoded_marks_that_break_its_rules() {
    let bad_colour = vec![Mark {
        needle: text("error"),
        colour: 9,
    }];
    let empty = vec![Mark {
        needle: text(""),
        colour: 1,
    }];
    let twice = vec![
        Mark {
            needle: text("error"),
            colour: 1,
        },
        Mark {
            needle: text("error"),
            colour: 2,
        },
    ];
    for marks in [bad_colour, empty, twice] {
        let bytes = postcard::to_stdvec(&marks).unwrap();
        assert!(
            postcard::from_bytes::<MarkSet>(&bytes).is_err(),
            "{marks:?}"
        );
    }
}
