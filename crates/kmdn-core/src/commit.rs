//! Commit on save inside a thread worktree (D27, D15). Only paths matching the allowed
//! globs are staged; everything else stays untracked or unstaged.

use std::path::Path;

use git2::{IndexAddOption, Oid, Repository, Signature};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::repo::RepoError;

/// Paths kmdn itself commits: documents, assets, config, and the two CI check files it writes.
pub const DEFAULT_ALLOWED: &[&str] = &[
    "**/*.md",
    "*.md",
    "**/assets/**",
    "assets/**",
    ".kmdn/**",
    ".github/workflows/kmdn-check.yml",
    ".gitlab-ci.yml",
];

pub fn allowed_set(globs: &[&str]) -> Result<GlobSet, globset::Error> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g)?);
    }
    b.build()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Author {
    pub name: String,
    pub email: String,
}

/// Stages every allowed path that is new, modified, or deleted, then commits.
/// Returns None when nothing allowed changed.
pub fn commit_allowed(
    worktree: &Path,
    message: &str,
    author: &Author,
    allowed: &GlobSet,
) -> Result<Option<Oid>, RepoError> {
    commit_allowed_except(
        worktree,
        message,
        author,
        allowed,
        &std::collections::HashSet::new(),
    )
}

/// Like `commit_allowed`, but leaves `exclude` (worktree-relative paths) out of the commit.
/// Used so pending agent edits stay uncommitted until the user accepts them (D15).
pub fn commit_allowed_except(
    worktree: &Path,
    message: &str,
    author: &Author,
    allowed: &GlobSet,
    exclude: &std::collections::HashSet<String>,
) -> Result<Option<Oid>, RepoError> {
    let repo = Repository::open(worktree)?;
    let (to_add, to_remove) =
        changed_paths(&repo, |p| allowed.is_match(p) && !exclude.contains(p))?;
    commit_staged(&repo, message, author, &to_add, &to_remove, &[])
}

/// Commits exactly `paths` (added, modified, or deleted) with `message` and optional trailers.
pub fn commit_paths(
    worktree: &Path,
    message: &str,
    author: &Author,
    paths: &[String],
    trailers: &[String],
) -> Result<Option<Oid>, RepoError> {
    let repo = Repository::open(worktree)?;
    let wanted: std::collections::HashSet<&str> = paths.iter().map(|s| s.as_str()).collect();
    let (to_add, to_remove) = changed_paths(&repo, |p| wanted.contains(p))?;
    commit_staged(&repo, message, author, &to_add, &to_remove, trailers)
}

/// Restores `paths` to their HEAD content, deleting files HEAD does not have.
pub fn revert_paths(worktree: &Path, paths: &[String]) -> Result<(), RepoError> {
    let repo = Repository::open(worktree)?;
    let head_tree = repo.head()?.peel_to_tree()?;
    let mut index = repo.index()?;
    for p in paths {
        let rel = Path::new(p);
        match head_tree.get_path(rel) {
            Ok(entry) => {
                let blob = repo.find_blob(entry.id())?;
                if let Some(parent) = worktree.join(rel).parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| git2::Error::from_str(&e.to_string()))?;
                }
                std::fs::write(worktree.join(rel), blob.content())
                    .map_err(|e| git2::Error::from_str(&e.to_string()))?;
                index.add_path(rel)?;
            }
            Err(_) => {
                let _ = std::fs::remove_file(worktree.join(rel));
                let _ = index.remove_path(rel);
            }
        }
    }
    index.write()?;
    Ok(())
}

fn changed_paths(
    repo: &Repository,
    keep: impl Fn(&str) -> bool,
) -> Result<(Vec<String>, Vec<String>), RepoError> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    let mut to_add = Vec::new();
    let mut to_remove = Vec::new();
    for s in statuses.iter() {
        let Some(p) = s.path() else { continue };
        if !keep(p) {
            continue;
        }
        let st = s.status();
        if st.intersects(git2::Status::WT_DELETED | git2::Status::INDEX_DELETED) {
            to_remove.push(p.to_string());
        } else if st.intersects(
            git2::Status::WT_NEW
                | git2::Status::WT_MODIFIED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE
                | git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED,
        ) {
            to_add.push(p.to_string());
        }
    }
    Ok((to_add, to_remove))
}

fn commit_staged(
    repo: &Repository,
    message: &str,
    author: &Author,
    to_add: &[String],
    to_remove: &[String],
    trailers: &[String],
) -> Result<Option<Oid>, RepoError> {
    if to_add.is_empty() && to_remove.is_empty() {
        return Ok(None);
    }
    let mut index = repo.index()?;
    // Start from HEAD so nothing staged by other tools rides along.
    index.read_tree(&repo.head()?.peel_to_tree()?)?;
    index.add_all(to_add.iter(), IndexAddOption::DEFAULT, None)?;
    for p in to_remove {
        index.remove_path(Path::new(p))?;
    }
    index.write()?;
    let tree_oid = index.write_tree()?;
    let head = repo.head()?.peel_to_commit()?;
    if head.tree_id() == tree_oid {
        return Ok(None);
    }
    let tree = repo.find_tree(tree_oid)?;
    let sig = Signature::now(&author.name, &author.email)?;
    let full = if trailers.is_empty() {
        message.to_string()
    } else {
        format!("{message}\n\n{}\n", trailers.join("\n"))
    };
    let oid = repo.commit(Some("HEAD"), &sig, &sig, &full, &tree, &[&head])?;
    Ok(Some(oid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repo;
    use crate::test_support::seeded_repo;

    #[test]
    fn commits_only_allowed_paths() {
        let (_d, root) = seeded_repo();
        let repo = Repo::open(&root).unwrap();
        let wt = repo
            .create_thread_worktree("a", "t", "refs/heads/main")
            .unwrap();
        std::fs::write(wt.path.join("doc.md"), "# Doc\n").unwrap();
        std::fs::create_dir_all(wt.path.join("assets/doc")).unwrap();
        std::fs::write(wt.path.join("assets/doc/a.png"), b"png").unwrap();
        std::fs::write(wt.path.join("notes.txt"), "no").unwrap();
        let author = Author {
            name: "A".into(),
            email: "a@x.io".into(),
        };
        let allowed = allowed_set(DEFAULT_ALLOWED).unwrap();

        let oid = commit_allowed(&wt.path, "Update doc", &author, &allowed).unwrap();
        assert!(oid.is_some());
        let wrepo = Repository::open(&wt.path).unwrap();
        let tree = wrepo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_path(Path::new("doc.md")).is_ok());
        assert!(tree.get_path(Path::new("assets/doc/a.png")).is_ok());
        assert!(tree.get_path(Path::new("notes.txt")).is_err());
        assert_eq!(
            wrepo
                .head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .author()
                .name(),
            Some("A")
        );

        // nothing allowed changed: no commit
        assert!(commit_allowed(&wt.path, "noop", &author, &allowed)
            .unwrap()
            .is_none());

        // deletion is committed too
        std::fs::remove_file(wt.path.join("doc.md")).unwrap();
        assert!(commit_allowed(&wt.path, "Remove doc", &author, &allowed)
            .unwrap()
            .is_some());
        let wrepo2 = Repository::open(&wt.path).unwrap();
        let tree = wrepo2.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_path(Path::new("doc.md")).is_err());
    }

    #[test]
    fn excluded_paths_stay_uncommitted_and_can_be_committed_or_reverted_separately() {
        let (_d, root) = seeded_repo();
        let repo = Repo::open(&root).unwrap();
        let wt = repo
            .create_thread_worktree("a", "t", "refs/heads/main")
            .unwrap();
        let author = Author {
            name: "A".into(),
            email: "a@x.io".into(),
        };
        let allowed = allowed_set(DEFAULT_ALLOWED).unwrap();
        std::fs::write(wt.path.join("human.md"), "# Human\n").unwrap();
        std::fs::write(wt.path.join("agent.md"), "# Agent\n").unwrap();
        std::fs::write(wt.path.join("README.md"), "# KB\nagent edit\n").unwrap();
        let pending: std::collections::HashSet<String> =
            ["agent.md".to_string(), "README.md".to_string()]
                .into_iter()
                .collect();

        commit_allowed_except(&wt.path, "Update human", &author, &allowed, &pending)
            .unwrap()
            .unwrap();
        let wrepo = Repository::open(&wt.path).unwrap();
        let tree = wrepo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_path(Path::new("human.md")).is_ok());
        assert!(
            tree.get_path(Path::new("agent.md")).is_err(),
            "pending agent file must not be committed"
        );
        assert_eq!(
            std::fs::read_to_string(wt.path.join("README.md")).unwrap(),
            "# KB\nagent edit\n",
            "pending edit stays on disk"
        );

        // accept one, revert the other
        let oid = commit_paths(
            &wt.path,
            "Agent (Claude): write agent doc",
            &author,
            &["agent.md".into()],
            &["Co-Authored-By: Claude <agent@kmdn.local>".into()],
        )
        .unwrap()
        .unwrap();
        let msg = wrepo
            .find_commit(oid)
            .unwrap()
            .message()
            .unwrap()
            .to_string();
        assert!(msg.starts_with("Agent (Claude): write agent doc\n\nCo-Authored-By: Claude"));
        revert_paths(&wt.path, &["README.md".into()]).unwrap();
        assert_eq!(
            std::fs::read_to_string(wt.path.join("README.md")).unwrap(),
            "# KB\n"
        );
        assert!(commit_allowed(&wt.path, "noop", &author, &allowed)
            .unwrap()
            .is_none());

        // reverting a new file deletes it
        std::fs::write(wt.path.join("scratch.md"), "x").unwrap();
        revert_paths(&wt.path, &["scratch.md".into()]).unwrap();
        assert!(!wt.path.join("scratch.md").exists());
    }
}
