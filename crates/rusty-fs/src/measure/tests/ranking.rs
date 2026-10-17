use crate::{
    CancelToken, FakeFs, FolderProfile, LARGEST_LIMIT, LargeFile, RANKING_LIMIT, folder_profile,
};

fn sized(count: u64) -> FakeFs {
    let fs = FakeFs::new();
    for index in 1..=count {
        fs.add_file(&format!("d/f{}/n{index}.bin", index % 4), index * 10);
    }
    fs
}

fn profile(fs: &FakeFs) -> FolderProfile {
    folder_profile(
        fs,
        &fs.location("d"),
        &CancelToken::new(),
        &mut |_: &FolderProfile| {},
    )
}

fn file(path: &str, bytes: u64) -> LargeFile {
    LargeFile {
        path: path.to_owned(),
        bytes,
    }
}

#[test]
fn the_ranking_keeps_a_reserve_beyond_the_ten_shown() {
    let profile = profile(&sized(30));
    assert_eq!(profile.ranking.files().len(), 30);
    assert_eq!(profile.largest, profile.ranking.files()[..LARGEST_LIMIT]);
    assert!(!profile.ranking.is_short());
}

#[test]
fn removing_a_ranked_file_promotes_the_next() {
    let mut ranking = profile(&sized(15)).ranking;
    assert_eq!(ranking.remove("f3/n15.bin"), Some(150));
    let top = ranking.top();
    assert_eq!(top.len(), LARGEST_LIMIT);
    assert_eq!(top[0], file("f2/n14.bin", 140));
    assert_eq!(top[9], file("f1/n5.bin", 50));
}

#[test]
fn a_cut_ranking_runs_short_only_when_removals_eat_the_reserve() {
    let total = RANKING_LIMIT as u64 + 5;
    let mut ranking = profile(&sized(total)).ranking;
    assert_eq!(ranking.files().len(), RANKING_LIMIT);
    let paths: Vec<String> = ranking.files().iter().map(|f| f.path.clone()).collect();
    for path in &paths[..RANKING_LIMIT - LARGEST_LIMIT] {
        ranking.remove(path);
    }
    assert!(!ranking.is_short());
    ranking.remove(&paths[RANKING_LIMIT - LARGEST_LIMIT]);
    assert!(ranking.is_short());
}

#[test]
fn a_file_shrunk_below_the_floor_leaves_a_cut_ranking() {
    let mut ranking = profile(&sized(RANKING_LIMIT as u64 + 5)).ranking;
    let top = ranking.files()[0].clone();
    ranking.offer(file(&top.path, 1));
    assert_eq!(ranking.size_of(&top.path), None);
    assert_eq!(ranking.files().len(), RANKING_LIMIT - 1);
}

#[test]
fn an_uncut_ranking_keeps_small_files() {
    let mut ranking = profile(&sized(5)).ranking;
    ranking.offer(file("f9/tiny.bin", 1));
    assert_eq!(ranking.size_of("f9/tiny.bin"), Some(1));
    assert!(!ranking.is_short());
}

#[test]
fn a_grown_file_moves_up_and_a_new_big_file_enters() {
    let mut ranking = profile(&sized(15)).ranking;
    ranking.offer(file("f1/n1.bin", 1_000));
    ranking.offer(file("new/huge.bin", 2_000));
    let top = ranking.top();
    assert_eq!(top[0], file("new/huge.bin", 2_000));
    assert_eq!(top[1], file("f1/n1.bin", 1_000));
    assert_eq!(ranking.files().len(), 16);
}

#[test]
fn renaming_a_folder_moves_its_ranked_paths_only() {
    let mut ranking = profile(&sized(8)).ranking;
    ranking.rename("f1", "g1", '/');
    assert_eq!(ranking.size_of("g1/n5.bin"), Some(50));
    assert_eq!(ranking.size_of("f1/n5.bin"), None);
    assert_eq!(ranking.size_of("f2/n6.bin"), Some(60));
    ranking.rename("g1/n5.bin", "g1/five.bin", '/');
    assert_eq!(ranking.size_of("g1/five.bin"), Some(50));
}
