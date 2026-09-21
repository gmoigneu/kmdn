//! One worktree per thread (D40). Branch `kmdn/<user>/<slug>`, worktree at
//! `<clone>.kmdn-worktrees/<slug>`. The main clone is never checked out elsewhere.

use std::path::{Path, PathBuf};

use git2::{BranchType, Repository, WorktreeAddOptions};
use serde::{Deserialize, Serialize};

use crate::repo::{Repo, RepoError};

pub const BRANCH_PREFIX: &str = "kmdn/";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreadWorktree {
    pub slug: String,
    pub branch: String,
    pub path: PathBuf,
    /// Unix seconds when the thread was published (merged). Set by kmdn, read for the Done group.
    #[serde(default)]
    pub merged_at: Option<u64>,
}

pub const MERGED_AFTER_DAYS: u64 = 7;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Lowercase ASCII slug, hyphen separated, max 48 chars, never empty.
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in input.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "change".to_string()
    } else {
        out
    }
}

pub fn branch_name(user: &str, slug: &str) -> String {
    format!("{BRANCH_PREFIX}{}/{}", slugify(user), slug)
}

/// `<clone>.kmdn-worktrees/` next to the clone directory.
pub fn worktrees_dir(clone_root: &Path) -> PathBuf {
    let name = clone_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "kb".into());
    clone_root
        .parent()
        .unwrap_or(clone_root)
        .join(format!("{name}.kmdn-worktrees"))
}

impl Repo {
    /// Creates a branch from `base_ref` (e.g. `refs/remotes/origin/main` or `refs/heads/main`)
    /// and a worktree checked out on it. Fails if the slug is taken.
    pub fn create_thread_worktree(
        &self,
        user: &str,
        slug: &str,
        base_ref: &str,
    ) -> Result<ThreadWorktree, RepoError> {
        let slug = slugify(slug);
        let branch = branch_name(user, &slug);
        let git = self.git();

        let base = git.find_reference(base_ref)?.peel_to_commit()?;
        let b = git.branch(&branch, &base, false)?;
        let reference = b.into_reference();

        let dir = worktrees_dir(self.root());
        std::fs::create_dir_all(&dir).map_err(|e| git2::Error::from_str(&e.to_string()))?;
        let path = dir.join(&slug);
        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        git.worktree(&slug, &path, Some(&opts))?;
        Ok(ThreadWorktree {
            slug,
            branch,
            path,
            merged_at: None,
        })
    }

    /// Every thread worktree: branches with the `kmdn/` prefix, plus adopted branches (D45),
    /// which keep their own name but live in kmdn's worktree directory.
    pub fn list_thread_worktrees(&self) -> Result<Vec<ThreadWorktree>, RepoError> {
        let git = self.git();
        let dir = worktrees_dir(self.root());
        let dir = dir.canonicalize().unwrap_or(dir);
        let mut out = Vec::new();
        for name in git.worktrees()?.iter().flatten() {
            let wt = git.find_worktree(name)?;
            let path = wt.path().to_path_buf();
            let branch = Repository::open(&path)
                .ok()
                .and_then(|r| {
                    r.head()
                        .ok()
                        .and_then(|h| h.shorthand().map(str::to_string))
                })
                .unwrap_or_default();
            // A worktree whose directory is gone or whose HEAD cannot be read is a stale git
            // entry, not a thread; it would only produce a thread that fails on every command.
            if branch.is_empty() || !path.is_dir() {
                continue;
            }
            let in_kmdn_dir = path
                .parent()
                .map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()) == dir)
                .unwrap_or(false);
            if branch.starts_with(BRANCH_PREFIX) || in_kmdn_dir {
                out.push(ThreadWorktree {
                    merged_at: self.thread_merged_at(name),
                    slug: name.to_string(),
                    branch,
                    path,
                });
            }
        }
        Ok(out)
    }

    /// The marker lives in the per-worktree git dir, so it travels with the worktree and
    /// disappears with it.
    fn merged_marker(&self, slug: &str) -> PathBuf {
        self.git()
            .path()
            .join("worktrees")
            .join(slug)
            .join("kmdn-merged")
    }

    /// Records that the thread's review was published (D49, 05-git publish).
    pub fn mark_thread_merged(&self, slug: &str) -> Result<(), RepoError> {
        let marker = self.merged_marker(slug);
        if marker.exists() {
            return Ok(());
        }
        std::fs::write(&marker, now_secs().to_string())
            .map_err(|e| git2::Error::from_str(&e.to_string()))?;
        Ok(())
    }

    pub fn thread_merged_at(&self, slug: &str) -> Option<u64> {
        std::fs::read_to_string(self.merged_marker(slug))
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    /// Removes worktrees published more than `days` ago (D49). Returns the slugs removed.
    pub fn remove_merged_older_than(&self, days: u64) -> Result<Vec<String>, RepoError> {
        let cutoff = now_secs().saturating_sub(days * 86_400);
        let mut removed = Vec::new();
        for t in self.list_thread_worktrees()? {
            if let Some(at) = t.merged_at {
                if at <= cutoff {
                    self.remove_thread_worktree(&t.slug, true)?;
                    removed.push(t.slug);
                }
            }
        }
        Ok(removed)
    }

    /// Removes the worktree directory and, for `kmdn/` branches, the branch. Adopted branches
    /// belong to the user and are kept. `force` discards uncommitted work.
    pub fn remove_thread_worktree(&self, slug: &str, force: bool) -> Result<(), RepoError> {
        let git = self.git();
        let wt = git.find_worktree(slug)?;
        let path = wt.path().to_path_buf();
        let branch = Repository::open(&path).ok().and_then(|r| {
            r.head()
                .ok()
                .and_then(|h| h.shorthand().map(str::to_string))
        });
        if !force {
            let inner = Repository::open(&path)?;
            let mut so = git2::StatusOptions::new();
            so.include_untracked(true);
            if !inner.statuses(Some(&mut so))?.is_empty() {
                return Err(git2::Error::from_str("worktree has uncommitted changes").into());
            }
        }
        if path.exists() {
            std::fs::remove_dir_all(&path).map_err(|e| git2::Error::from_str(&e.to_string()))?;
        }
        let mut po = git2::WorktreePruneOptions::new();
        po.valid(true).working_tree(true);
        wt.prune(Some(&mut po))?;
        if let Some(b) = branch.filter(|b| b.starts_with(BRANCH_PREFIX)) {
            if let Ok(mut br) = git.find_branch(&b, BranchType::Local) {
                br.delete()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_repo() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("kb");
        std::fs::create_dir(&root).unwrap();
        {
            let git = Repository::init_opts(
                &root,
                git2::RepositoryInitOptions::new().initial_head("main"),
            )
            .unwrap();
            std::fs::write(root.join("README.md"), "# KB\n").unwrap();
            let mut idx = git.index().unwrap();
            idx.add_path(Path::new("README.md")).unwrap();
            idx.write().unwrap();
            let tree = git.find_tree(idx.write_tree().unwrap()).unwrap();
            let sig = git2::Signature::now("t", "t@example.com").unwrap();
            git.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }
        let repo = Repo::open(&root).unwrap();
        (dir, repo)
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("Deploy Runbook: v2!"), "deploy-runbook-v2");
        assert_eq!(slugify("---"), "change");
        assert_eq!(
            branch_name("G Moigneu", "fix-typo"),
            "kmdn/g-moigneu/fix-typo"
        );
    }

    #[test]
    fn worktree_lifecycle() {
        let (_dir, repo) = seeded_repo();
        let wt = repo
            .create_thread_worktree("alice", "Fix Typo", "refs/heads/main")
            .unwrap();
        assert!(wt.path.join("README.md").exists());
        assert_eq!(wt.branch, "kmdn/alice/fix-typo");
        assert_eq!(
            worktrees_dir(repo.root()).file_name().unwrap(),
            "kb.kmdn-worktrees"
        );

        // main clone untouched
        assert_eq!(repo.head_branch().unwrap().as_deref(), Some("main"));

        let list = repo.list_thread_worktrees().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].slug, "fix-typo");

        // dirty worktree refuses non-forced removal
        std::fs::write(wt.path.join("x.md"), "x").unwrap();
        assert!(repo.remove_thread_worktree("fix-typo", false).is_err());
        repo.remove_thread_worktree("fix-typo", true).unwrap();
        assert!(!wt.path.exists());
        assert!(repo.list_thread_worktrees().unwrap().is_empty());
        assert!(repo
            .git()
            .find_branch("kmdn/alice/fix-typo", BranchType::Local)
            .is_err());
    }

    #[test]
    fn two_worktrees_do_not_cross_talk() {
        let (_dir, repo) = seeded_repo();
        let a = repo
            .create_thread_worktree("a", "one", "refs/heads/main")
            .unwrap();
        let b = repo
            .create_thread_worktree("b", "two", "refs/heads/main")
            .unwrap();
        std::fs::write(a.path.join("only-a.md"), "a").unwrap();
        assert!(!b.path.join("only-a.md").exists());
        assert!(!repo.root().join("only-a.md").exists());
    }

    #[test]
    fn stale_worktrees_are_not_threads() {
        let (_d, repo) = seeded_repo();
        let wt = repo
            .create_thread_worktree("alice", "gone", "refs/heads/main")
            .unwrap();
        assert_eq!(repo.list_thread_worktrees().unwrap().len(), 1);
        std::fs::remove_dir_all(&wt.path).unwrap();
        assert!(repo.list_thread_worktrees().unwrap().is_empty());
    }

    #[test]
    fn merged_marker_and_cleanup() {
        let (_dir, repo) = seeded_repo();
        let wt = repo
            .create_thread_worktree("a", "done", "refs/heads/main")
            .unwrap();
        assert_eq!(repo.list_thread_worktrees().unwrap()[0].merged_at, None);
        repo.mark_thread_merged(&wt.slug).unwrap();
        let at = repo.list_thread_worktrees().unwrap()[0].merged_at.unwrap();
        assert!(at > 0);
        // fresh merge: kept
        assert!(repo.remove_merged_older_than(7).unwrap().is_empty());
        // pretend it was published long ago
        std::fs::write(repo.merged_marker(&wt.slug), (at - 8 * 86_400).to_string()).unwrap();
        assert_eq!(
            repo.remove_merged_older_than(7).unwrap(),
            vec!["done".to_string()]
        );
        assert!(repo.list_thread_worktrees().unwrap().is_empty());
    }
}
