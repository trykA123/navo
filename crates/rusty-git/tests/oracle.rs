use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Read;
use std::path::Path;

use rusty_git::{
    BlobReader, Choice, CliGit, EngineKind, EngineSetting, GitFound, GitSource, GixGit, ObjectId,
    RepoFormat, TreeEntry, TreeKind, Work, choose,
};

#[cfg(test)]
#[path = "oracle/git.rs"]
mod git;

#[cfg(test)]
mod oracle {
    use super::git;
    use super::*;

    const BLOBS_PER_TREE: usize = 3;
    const MAX_BLOBS: usize = 200;
    const MAX_BLOB_BYTES: u64 = 1024 * 1024;

    #[test]
    fn oracle_gix_and_cli_agree_on_git_repositories() {
        let env_repos = git::env_repositories();
        let Some(git_path) = git::find_git() else {
            if env_repos.is_empty() {
                git::say("skipping: git was not found on PATH");
                return;
            }
            panic!("oracle: FERRITE_ORACLE_REPOS is set but git was not found on PATH");
        };
        let (major, minor) =
            git::git_version(&git_path).unwrap_or_else(|error| panic!("oracle: {error}"));
        if (major, minor) < (2, 36) {
            panic!(
                "oracle: git {major}.{minor} is older than 2.36{}",
                if env_repos.is_empty() {
                    ""
                } else {
                    " (FERRITE_ORACLE_REPOS is set)"
                }
            );
        }
        let git_found = GitFound::version(major, minor);
        let mut repos = env_repos;
        repos.push(git::worktree_root(&git_path));
        let mut seen = HashSet::new();
        repos.retain(|path| {
            let canonical = std::fs::canonicalize(path)
                .unwrap_or_else(|error| panic!("oracle: repository {}: {error}", path.display()));
            seen.insert(canonical)
        });
        for repo in repos {
            compare_repository(&git_path, git_found, &repo);
        }
    }

    fn compare_repository(git_path: &Path, git_found: GitFound, repo: &Path) {
        let format = RepoFormat::read(repo).unwrap_or_else(|error| {
            panic!("oracle: reading the format of {}: {error}", repo.display())
        });
        let (cli, gix) = open_engines(repo);
        let Some(head) = cli.resolve("HEAD").unwrap_or_else(|error| {
            panic!("oracle: resolving HEAD in {}: {error}", repo.display())
        }) else {
            panic!("oracle: {} has an unborn HEAD", repo.display());
        };
        let (commits, merges) = git::commit_list(git_path, repo, &hex(&head));
        if commits.is_empty() {
            panic!("oracle: {} has no commits", repo.display());
        }
        let mut checked = commits;
        for merge in &merges {
            if !checked.contains(merge) {
                checked.push(merge.clone());
            }
        }
        let mut comparison = Comparison {
            git: git_path,
            repo,
            gix: &gix,
            cli: &cli,
            choices: merge_base_choices(&format, git_found),
            sampled: HashSet::new(),
            stats: Stats {
                commits: checked.len(),
                ..Stats::default()
            },
        };
        for commit in &checked {
            comparison.tree(commit);
            comparison.merge_base(commit, "HEAD");
        }
        for merge in &merges {
            for (left, right) in git::merge_parent_pairs(git_path, repo, merge) {
                comparison.merge_base(&left, &right);
            }
        }
        comparison.finish();
    }

    fn open_engines(repo: &Path) -> (CliGit, GixGit) {
        let cli = CliGit::new(repo)
            .unwrap_or_else(|error| panic!("oracle: opening {} with git: {error}", repo.display()));
        let gix = GixGit::new(repo)
            .unwrap_or_else(|error| panic!("oracle: opening {} with gix: {error}", repo.display()));
        (cli, gix)
    }

    fn merge_base_choices(
        format: &RepoFormat,
        git_found: GitFound,
    ) -> [(EngineSetting, Choice); 3] {
        [EngineSetting::Git, EngineSetting::Auto, EngineSetting::Gix].map(|setting| {
            let choice = choose(setting, format, git_found).for_work(Work::MergeBase, git_found);
            assert_eq!(
                choice.engine,
                EngineKind::Cli,
                "merge-base must route to the CLI when Git is usable"
            );
            (setting, choice)
        })
    }

    #[derive(Default)]
    struct Stats {
        commits: usize,
        trees: usize,
        blobs: usize,
        merge_bases: usize,
        has_blobs: bool,
    }

    struct Comparison<'a> {
        git: &'a Path,
        repo: &'a Path,
        gix: &'a GixGit,
        cli: &'a CliGit,
        choices: [(EngineSetting, Choice); 3],
        sampled: HashSet<ObjectId>,
        stats: Stats,
    }

    impl Comparison<'_> {
        fn tree(&mut self, commit: &str) {
            let cli_tree = self.cli.list_tree(commit).expect("git tree listing");
            let gix_tree = self.gix.list_tree(commit).expect("gix tree listing");
            if gix_tree != cli_tree {
                report_tree(commit, &gix_tree, &cli_tree);
            }
            self.stats.trees += 1;
            self.blobs(commit, &cli_tree);
        }

        fn blobs(&mut self, commit: &str, tree: &[TreeEntry]) {
            let candidates: Vec<&TreeEntry> = tree
                .iter()
                .filter(|entry| matches!(entry.kind, TreeKind::File | TreeKind::Link))
                .filter(|entry| entry.size.is_some_and(|size| size <= MAX_BLOB_BYTES))
                .filter(|entry| !self.sampled.contains(&entry.id))
                .collect();
            self.stats.has_blobs |= !candidates.is_empty();
            let remaining = MAX_BLOBS.saturating_sub(self.sampled.len());
            for index in sample_indexes(candidates.len(), BLOBS_PER_TREE)
                .into_iter()
                .take(remaining)
            {
                let entry = candidates[index];
                self.sampled.insert(entry.id);
                self.blob(commit, entry);
            }
        }

        fn blob(&mut self, commit: &str, entry: &TreeEntry) {
            let gix_bytes = read_all(self.gix.read_blob(&entry.id).expect("gix blob"));
            let cli_bytes = read_all(self.cli.read_blob(&entry.id).expect("git blob"));
            if gix_bytes != cli_bytes {
                report_blob(commit, entry, &gix_bytes, &cli_bytes);
            }
            self.stats.blobs += 1;
        }

        fn merge_base(&mut self, left: &str, right: &str) {
            let cli_base = self.cli.merge_base(left, right).unwrap_or_else(|error| {
                panic!(
                    "oracle: git merge-base {left} {right} in {}: {error}",
                    self.repo.display()
                )
            });
            let gix_base = self.gix.merge_base(left, right).unwrap_or_else(|error| {
                panic!(
                    "oracle: gix merge-base {left} {right} in {}: {error}",
                    self.repo.display()
                )
            });
            let bases = git::merge_base_all(self.git, self.repo, left, right);
            if bases.len() > 1 {
                let mut all: Vec<&String> = bases.iter().collect();
                all.sort();
                git::say(&format!(
                    "{}: criss-cross {left} {right}: gix {}, cli {}, all {all:?}",
                    self.repo.display(),
                    base_text(&gix_base),
                    base_text(&cli_base)
                ));
            }
            match &gix_base {
                Some(id) if !bases.contains(&hex(id)) => panic!(
                    "oracle: gix merge-base for {left} {right} in {} is {} but git --all is {bases:?}",
                    self.repo.display(),
                    hex(id)
                ),
                None if !bases.is_empty() => panic!(
                    "oracle: gix found no merge-base for {left} {right} in {} but git --all is {bases:?}",
                    self.repo.display()
                ),
                _ => {}
            }
            for (setting, choice) in &self.choices {
                let effective = match choice.engine {
                    EngineKind::Cli => &cli_base,
                    EngineKind::Gix => &gix_base,
                };
                assert_eq!(
                    effective,
                    &cli_base,
                    "routed merge-base for {setting:?} at {left} {right} in {}",
                    self.repo.display()
                );
            }
            self.stats.merge_bases += 1;
        }

        fn finish(&self) {
            let stats = &self.stats;
            if stats.commits == 0 || stats.trees == 0 {
                panic!(
                    "oracle: {} compared no commits or trees",
                    self.repo.display()
                );
            }
            if stats.trees != stats.commits {
                panic!(
                    "oracle: {} compared {} trees for {} commits",
                    self.repo.display(),
                    stats.trees,
                    stats.commits
                );
            }
            if stats.has_blobs && stats.blobs == 0 {
                panic!(
                    "oracle: {} has blobs but compared none",
                    self.repo.display()
                );
            }
            if stats.merge_bases == 0 {
                panic!("oracle: {} compared no merge-bases", self.repo.display());
            }
            git::say(&format!(
                "{}: {} commits, {} trees, {} blobs, {} merge-bases",
                self.repo.display(),
                stats.commits,
                stats.trees,
                stats.blobs,
                stats.merge_bases
            ));
        }
    }

    fn report_tree(commit: &str, gix: &[TreeEntry], git: &[TreeEntry]) -> ! {
        if let Some((index, (left, right))) = gix
            .iter()
            .zip(git)
            .enumerate()
            .find(|(_, (left, right))| left != right)
        {
            panic!(
                "oracle disagrees on tree {commit}, entry {index}: gix {} (kind {:?}, id {:?}, size {:?}), git {} (kind {:?}, id {:?}, size {:?})",
                left.path,
                left.kind,
                left.id,
                left.size,
                right.path,
                right.kind,
                right.id,
                right.size
            );
        }
        panic!(
            "oracle disagrees on tree {commit}: gix has {} entries, git has {}",
            gix.len(),
            git.len()
        );
    }

    fn report_blob(commit: &str, entry: &TreeEntry, gix: &[u8], git: &[u8]) -> ! {
        let offset = gix.iter().zip(git).position(|(left, right)| left != right);
        panic!(
            "oracle disagrees on blob {} at {commit}: gix {} bytes, git {} bytes, first difference {offset:?}",
            entry.path,
            gix.len(),
            git.len()
        );
    }

    fn sample_indexes(len: usize, count: usize) -> Vec<usize> {
        if len <= count {
            return (0..len).collect();
        }
        (0..count)
            .map(|index| index * (len - 1) / (count - 1))
            .collect()
    }

    fn read_all(mut reader: BlobReader) -> Vec<u8> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    }

    fn hex(id: &ObjectId) -> String {
        let mut text = String::with_capacity(id.byte_len() * 2);
        for byte in id.as_bytes() {
            write!(text, "{byte:02x}").unwrap_or_default();
        }
        text
    }

    fn base_text(base: &Option<ObjectId>) -> String {
        base.as_ref().map_or_else(|| "none".to_owned(), hex)
    }
}
