//! What a thread changed relative to the default branch: file list with old and new content
//! for the rendered diff (D44). Compares the merge base with the worktree's working directory,
//! so uncommitted saves are included.

use std::path::Path;

use git2::{DiffOptions, Repository};
use serde::{Deserialize, Serialize};

use crate::repo::RepoError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    /// Text content, None for binary or absent side.
    pub old: Option<String>,
    pub new: Option<String>,
    pub binary: bool,
}

fn text_of(bytes: &[u8]) -> Option<String> {
    if bytes.contains(&0) {
        None
    } else {
        Some(String::from_utf8_lossy(bytes).to_string())
    }
}

/// Builds a FileChange from a delta and the two sides' text.
fn delta_to_change(
    delta: &git2::DiffDelta<'_>,
    old: Option<String>,
    new: Option<String>,
) -> Option<FileChange> {
    let status = match delta.status() {
        git2::Delta::Added | git2::Delta::Untracked => ChangeStatus::Added,
        git2::Delta::Deleted => ChangeStatus::Deleted,
        git2::Delta::Renamed => ChangeStatus::Renamed,
        git2::Delta::Modified | git2::Delta::Typechange => ChangeStatus::Modified,
        _ => return None,
    };
    let new_path = delta
        .new_file()
        .path()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let old_path = delta
        .old_file()
        .path()
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    let binary = delta.flags().is_binary()
        || (old.is_none() && status != ChangeStatus::Added)
        || (new.is_none() && status != ChangeStatus::Deleted);
    Some(FileChange {
        path: if status == ChangeStatus::Deleted {
            old_path.clone().unwrap_or(new_path.clone())
        } else {
            new_path
        },
        old_path: if status == ChangeStatus::Renamed {
            old_path
        } else {
            None
        },
        status,
        old,
        new,
        binary,
    })
}

fn status_of(delta: &git2::DiffDelta<'_>) -> Option<ChangeStatus> {
    match delta.status() {
        git2::Delta::Added | git2::Delta::Untracked => Some(ChangeStatus::Added),
        git2::Delta::Deleted => Some(ChangeStatus::Deleted),
        git2::Delta::Renamed => Some(ChangeStatus::Renamed),
        git2::Delta::Modified | git2::Delta::Typechange => Some(ChangeStatus::Modified),
        _ => None,
    }
}

/// Changes in `worktree` versus its merge base with `base_ref` (e.g. `refs/remotes/origin/main`).
pub fn thread_changes(worktree: &Path, base_ref: &str) -> Result<Vec<FileChange>, RepoError> {
    let repo = Repository::open(worktree)?;
    let head = repo.head()?.peel_to_commit()?;
    let base_commit = repo.find_reference(base_ref)?.peel_to_commit()?;
    let base_oid = repo.merge_base(head.id(), base_commit.id())?;
    let base_tree = repo.find_commit(base_oid)?.tree()?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
    let mut diff = repo.diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))?;
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))?;

    let workdir = repo
        .workdir()
        .ok_or_else(|| git2::Error::from_str("no workdir"))?;
    let mut out = Vec::new();
    for delta in diff.deltas() {
        let Some(status) = status_of(&delta) else {
            continue;
        };
        let new_path = delta
            .new_file()
            .path()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let old = if status == ChangeStatus::Added {
            None
        } else {
            base_tree
                .get_path(delta.old_file().path().unwrap_or(Path::new("")))
                .ok()
                .and_then(|e| repo.find_blob(e.id()).ok())
                .and_then(|b| text_of(b.content()))
        };
        let new = if status == ChangeStatus::Deleted {
            None
        } else {
            std::fs::read(workdir.join(&new_path))
                .ok()
                .and_then(|b| text_of(&b))
        };
        out.extend(delta_to_change(&delta, old, new));
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Changes between two commits, e.g. a PR's merge base and head, for the review layout (D48).
pub fn changes_between(
    repo: &Repository,
    base_oid: git2::Oid,
    head_oid: git2::Oid,
) -> Result<Vec<FileChange>, RepoError> {
    let head_commit = repo.find_commit(head_oid)?;
    let merge_base = repo.merge_base(base_oid, head_oid).unwrap_or(base_oid);
    let base_tree = repo.find_commit(merge_base)?.tree()?;
    let head_tree = head_commit.tree()?;
    let mut diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))?;
    let blob_text = |tree: &git2::Tree, p: Option<&Path>| -> Option<String> {
        let p = p?;
        let e = tree.get_path(p).ok()?;
        let b = repo.find_blob(e.id()).ok()?;
        text_of(b.content())
    };
    let mut out = Vec::new();
    for delta in diff.deltas() {
        let Some(status) = status_of(&delta) else {
            continue;
        };
        let old = if status == ChangeStatus::Added {
            None
        } else {
            blob_text(&base_tree, delta.old_file().path())
        };
        let new = if status == ChangeStatus::Deleted {
            None
        } else {
            blob_text(&head_tree, delta.new_file().path())
        };
        out.extend(delta_to_change(&delta, old, new));
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Fetches a PR head branch into `refs/remotes/origin/<branch>` and returns its oid.
pub fn fetch_branch(
    repo: &Repository,
    branch: &str,
    token: Option<&crate::sync::Token>,
) -> Result<git2::Oid, RepoError> {
    let mut r = repo.find_remote("origin")?;
    let mut fo = git2::FetchOptions::new();
    fo.remote_callbacks(crate::sync::callbacks(token));
    let spec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    r.fetch(&[spec.as_str()], Some(&mut fo), None)?;
    repo.find_reference(&format!("refs/remotes/origin/{branch}"))?
        .target()
        .ok_or_else(|| git2::Error::from_str("branch has no target").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::{allowed_set, commit_allowed, Author, DEFAULT_ALLOWED};
    use crate::repo::Repo;
    use crate::test_support::{commit_on_origin, seeded_repo_with_origin};

    #[test]
    fn lists_committed_and_uncommitted_changes_against_merge_base() {
        let (_d, root, upstream) = seeded_repo_with_origin();
        let repo = Repo::open(&root).unwrap();
        let wt = repo
            .create_thread_worktree("a", "t", "refs/remotes/origin/main")
            .unwrap();
        let allowed = allowed_set(DEFAULT_ALLOWED).unwrap();
        let author = Author {
            name: "A".into(),
            email: "a@x.io".into(),
        };

        std::fs::write(wt.path.join("README.md"), "# KB\nmore\n").unwrap();
        std::fs::write(wt.path.join("new.md"), "# New\n").unwrap();
        commit_allowed(&wt.path, "c1", &author, &allowed).unwrap();
        std::fs::write(wt.path.join("draft.md"), "# Draft, unsaved to git\n").unwrap();

        // main moves on independently; the diff must stay relative to the merge base
        commit_on_origin(&upstream, "elsewhere.md", "# E\n", "main e");
        crate::sync::fetch(repo.git(), "origin", None).unwrap();

        let ch = thread_changes(&wt.path, "refs/remotes/origin/main").unwrap();
        let paths: Vec<(&str, ChangeStatus)> =
            ch.iter().map(|c| (c.path.as_str(), c.status)).collect();
        assert_eq!(
            paths,
            vec![
                ("README.md", ChangeStatus::Modified),
                ("draft.md", ChangeStatus::Added),
                ("new.md", ChangeStatus::Added)
            ]
        );
        assert_eq!(ch[0].old.as_deref(), Some("# KB\n"));
        assert_eq!(ch[0].new.as_deref(), Some("# KB\nmore\n"));
        assert!(ch[1].old.is_none());
        assert!(!ch.iter().any(|c| c.path == "elsewhere.md"));
    }
}
