//! The main clone as seen by kmdn (D45): dirty state or a non-default HEAD shows as a
//! read-only "Local changes" thread. Move to new thread stashes and applies into a worktree.

use git2::{Repository, StashApplyOptions, StashFlags};
use serde::{Deserialize, Serialize};

use crate::repo::{Repo, RepoError};
use crate::worktree::{ThreadWorktree, BRANCH_PREFIX};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalChanges {
    pub head_branch: Option<String>,
    pub on_default_branch: bool,
    pub dirty_paths: Vec<String>,
    /// HEAD is on a branch kmdn does not manage.
    pub foreign_branch: Option<String>,
    pub operation_in_progress: Option<String>,
}

impl LocalChanges {
    pub fn is_clean(&self) -> bool {
        self.on_default_branch
            && self.dirty_paths.is_empty()
            && self.operation_in_progress.is_none()
    }
}

pub fn inspect(repo: &Repo) -> Result<LocalChanges, RepoError> {
    let head = repo.head_branch()?;
    let default = repo.default_branch()?;
    let on_default = head.as_deref() == Some(default.as_str());
    let foreign = match &head {
        Some(b) if !on_default && !b.starts_with(BRANCH_PREFIX) => Some(b.clone()),
        _ => None,
    };
    let op = match repo.git().state() {
        git2::RepositoryState::Clean => None,
        other => Some(format!("{other:?}").to_lowercase()),
    };
    Ok(LocalChanges {
        head_branch: head,
        on_default_branch: on_default,
        dirty_paths: repo.dirty_paths()?,
        foreign_branch: foreign,
        operation_in_progress: op,
    })
}

/// Stashes the main clone's changes (including untracked), creates a thread from the default
/// branch, applies the stash into its worktree, and drops the stash. The main clone ends clean.
pub fn move_to_new_thread(
    repo: &Repo,
    user: &str,
    slug: &str,
) -> Result<ThreadWorktree, RepoError> {
    let root = repo.root().to_path_buf();
    let default = repo.default_branch()?;
    {
        // Stash needs a mutable Repository handle; open a second one on the same path.
        let mut main = Repository::open(&root)?;
        let sig = main
            .signature()
            .unwrap_or_else(|_| git2::Signature::now("kmdn", "kmdn@localhost").unwrap());
        main.stash_save(
            &sig,
            "kmdn: move to new thread",
            Some(StashFlags::INCLUDE_UNTRACKED),
        )?;
    }
    let wt = repo.create_thread_worktree(user, slug, &format!("refs/heads/{default}"))?;
    let mut wrepo = Repository::open(&wt.path)?;
    let mut opts = StashApplyOptions::new();
    opts.reinstantiate_index();
    // refs/stash lives in the common dir, shared by all worktrees.
    wrepo.stash_apply(0, Some(&mut opts))?;
    let mut main = Repository::open(&root)?;
    main.stash_drop(0)?;
    Ok(wt)
}

/// Registers an existing non-kmdn branch as a thread by giving it a worktree. Never renames.
pub fn adopt_branch(repo: &Repo, branch: &str) -> Result<ThreadWorktree, RepoError> {
    let git = repo.git();
    let b = git.find_branch(branch, git2::BranchType::Local)?;
    let reference = b.into_reference();
    let slug = crate::worktree::slugify(branch);
    let dir = crate::worktree::worktrees_dir(repo.root());
    std::fs::create_dir_all(&dir).map_err(|e| git2::Error::from_str(&e.to_string()))?;
    let path = dir.join(&slug);
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    git.worktree(&slug, &path, Some(&opts))?;
    Ok(ThreadWorktree {
        slug,
        branch: branch.to_string(),
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::seeded_repo;

    #[test]
    fn inspect_and_move() {
        let (_d, root) = seeded_repo();
        let repo = Repo::open(&root).unwrap();
        assert!(inspect(&repo).unwrap().is_clean());

        std::fs::write(root.join("README.md"), "# KB\nedited in vscode\n").unwrap();
        std::fs::write(root.join("new.md"), "# New\n").unwrap();
        let lc = inspect(&repo).unwrap();
        assert!(!lc.is_clean());
        assert_eq!(lc.dirty_paths.len(), 2);

        let wt = move_to_new_thread(&repo, "dev", "from vscode").unwrap();
        assert_eq!(
            std::fs::read_to_string(wt.path.join("README.md")).unwrap(),
            "# KB\nedited in vscode\n"
        );
        assert!(wt.path.join("new.md").exists());
        // main clone is clean again and untouched
        assert!(inspect(&repo).unwrap().is_clean());
        assert_eq!(
            std::fs::read_to_string(root.join("README.md")).unwrap(),
            "# KB\n"
        );
        assert!(!root.join("new.md").exists());
        assert!(
            Repository::open(&root)
                .unwrap()
                .reflog("refs/stash")
                .map(|r| r.len())
                .unwrap_or(0)
                == 0
        );
    }

    #[test]
    fn adopt_foreign_branch() {
        let (_d, root) = seeded_repo();
        let repo = Repo::open(&root).unwrap();
        let head = repo.git().head().unwrap().peel_to_commit().unwrap();
        repo.git()
            .branch("feature/docs-refresh", &head, false)
            .unwrap();
        repo.git()
            .set_head("refs/heads/feature/docs-refresh")
            .unwrap();
        let lc = inspect(&repo).unwrap();
        assert_eq!(lc.foreign_branch.as_deref(), Some("feature/docs-refresh"));

        repo.git().set_head("refs/heads/main").unwrap();
        let wt = adopt_branch(&repo, "feature/docs-refresh").unwrap();
        assert_eq!(wt.branch, "feature/docs-refresh");
        assert!(wt.path.join("README.md").exists());
        // The adopted branch is a thread like any other, under its own name (D45).
        let listed = repo.list_thread_worktrees().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].branch, "feature/docs-refresh");
        assert_eq!(listed[0].slug, wt.slug);
        // Abandoning it removes the worktree but keeps the user's branch.
        repo.remove_thread_worktree(&wt.slug, true).unwrap();
        assert!(!wt.path.exists());
        assert!(repo
            .git()
            .find_branch("feature/docs-refresh", git2::BranchType::Local)
            .is_ok());
        assert!(repo.list_thread_worktrees().unwrap().is_empty());
    }
}
