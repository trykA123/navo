use super::*;

#[test]
fn saved_badges_come_back_oldest_first_and_can_be_removed() {
    let mut cache = mem();
    cache.put_git_badge("a", b"1").unwrap();
    cache.put_git_badge("b", b"2").unwrap();
    cache.put_git_badge("a", b"3").unwrap();
    let all = cache.git_badges().unwrap();
    assert_eq!(
        all,
        vec![
            ("b".to_owned(), b"2".to_vec()),
            ("a".to_owned(), b"3".to_vec())
        ]
    );
    assert!(cache.remove_git_badge("a").unwrap());
    assert_eq!(cache.git_badges().unwrap().len(), 1);
}

#[test]
fn saved_badges_survive_a_reopen_and_stay_inside_their_budget() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cache.db");
    {
        let (mut cache, _) = CacheDb::open(&path).unwrap();
        for index in 0..40 {
            let value = vec![0_u8; 128 * 1024];
            cache
                .put_git_badge(&format!("repo-{index}"), &value)
                .unwrap();
        }
    }
    let (cache, _) = CacheDb::open(&path).unwrap();
    let kept = cache.git_badges().unwrap();
    assert!(!kept.is_empty() && kept.len() < 40);
    assert!(cache.used_bytes("git-badges").unwrap() <= GIT_BADGES_BUDGET);
    assert!(kept.iter().any(|(key, _)| key == "repo-39"));
}
