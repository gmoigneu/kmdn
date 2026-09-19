//! Commit on save inside a thread worktree (D27, D15). Only paths matching the allowed
//! globs are staged; everything else stays untracked or unstaged.

use std::path::Path;

use git2::{IndexAddOption, Oid, Repository, Signature};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::repo::RepoError;

pub const DEFAULT_ALLOWED: &[&str] = &["**/*.md", "*.md", "**/assets/**", "assets/**", ".kmdn/**"];

pub fn allowed_set(globs: &[&str]) -> Result<GlobSet, globset::Error> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g)?);
    }
    b.build()
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    let repo = Repository::open(worktree)?;
    let mut index = repo.index()?;

    // Collect candidate paths from status, filter by globs, then stage adds and removals.
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    let mut to_add = Vec::new();
    let mut to_remove = Vec::new();
    for s in statuses.iter() {
        let Some(p) = s.path() else { continue };
        if !allowed.is_match(p) {
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
    if to_add.is_empty() && to_remove.is_empty() {
        return Ok(None);
    }
    index.add_all(to_add.iter(), IndexAddOption::DEFAULT, None)?;
    for p in &to_remove {
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
    let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head])?;
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
}
