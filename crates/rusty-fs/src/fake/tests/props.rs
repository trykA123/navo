use proptest::prelude::*;

use super::*;

fn build(parents: &[usize], files: &[bool]) -> (FakeFs, Vec<Vec<String>>) {
    let fs = FakeFs::new();
    let mut paths: Vec<String> = Vec::new();
    let mut depths: Vec<usize> = Vec::new();
    let mut children: Vec<Vec<String>> = vec![Vec::new(); parents.len() + 1];
    for (i, (&pick, &file)) in parents.iter().zip(files).enumerate() {
        let parent = folder_index(&paths, &depths, files, pick);
        let name = format!("n{i}");
        let base = parent.map_or(String::new(), |p| format!("{}/", paths[p]));
        let path = format!("{base}{name}");
        if file {
            fs.add_file(&path, i as u64);
        } else {
            fs.add_folder(&path);
        }
        children[parent.map_or(0, |p| p + 1)].push(name);
        depths.push(parent.map_or(1, |p| depths[p] + 1));
        paths.push(path);
    }
    (fs, children)
}

fn folder_index(paths: &[String], depths: &[usize], files: &[bool], pick: usize) -> Option<usize> {
    let folders: Vec<usize> = (0..paths.len())
        .filter(|&j| !files[j] && depths[j] < 4)
        .collect();
    match pick % (folders.len() + 1) {
        0 => None,
        k => Some(folders[k - 1]),
    }
}

fn folder_paths(parents: &[usize], files: &[bool]) -> Vec<(usize, String)> {
    let mut paths: Vec<String> = Vec::new();
    let mut depths: Vec<usize> = Vec::new();
    let mut out = vec![(0, String::new())];
    for (i, &pick) in parents.iter().enumerate() {
        let parent = folder_index(&paths, &depths, files, pick);
        let base = parent.map_or(String::new(), |p| format!("{}/", paths[p]));
        let path = format!("{base}n{i}");
        if !files[i] {
            out.push((i + 1, path.clone()));
        }
        depths.push(parent.map_or(1, |p| depths[p] + 1));
        paths.push(path);
    }
    out
}

fn tree() -> impl Strategy<Value = (Vec<usize>, Vec<bool>)> {
    (0..=50usize).prop_flat_map(|n| {
        (
            prop::collection::vec(any::<usize>(), n),
            prop::collection::vec(any::<bool>(), n),
        )
    })
}

proptest! {
    #[test]
    fn batches_concatenate_to_the_children((parents, files) in tree(), size in 1..=10usize) {
        let (fs, children) = build(&parents, &files);
        let fs = fs.with_batch_size(size);
        for (index, path) in folder_paths(&parents, &files) {
            prop_assert_eq!(names(&list_all(&fs, &path).1), children[index].clone());
        }
    }

    #[test]
    fn only_the_final_batch_is_last((parents, files) in tree(), size in 1..=10usize) {
        let (fs, _) = build(&parents, &files);
        let fs = fs.with_batch_size(size);
        for (_, path) in folder_paths(&parents, &files) {
            let lasts: Vec<bool> = list_all(&fs, &path).1.iter().map(|b| b.last).collect();
            let mut expected = vec![false; lasts.len().saturating_sub(1)];
            expected.push(true);
            prop_assert_eq!(lasts, expected);
        }
    }
}
