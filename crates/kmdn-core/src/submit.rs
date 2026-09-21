//! Submit a thread for review (D26, D57, D58, thread lifecycle step 4 in 05-git-and-review.md):
//! run checks, regenerate AGENTS.md, commit, push with lease, create or update the PR,
//! post the condensed agent log as one comment.

use std::path::Path;

use git2::{Oid, Repository};
use serde::{Deserialize, Serialize};

use crate::checks;
use crate::commit::{allowed_set, commit_allowed, Author, DEFAULT_ALLOWED};
use crate::diff::{thread_changes, ChangeStatus, FileChange};
use crate::index;
use crate::provider::{Comment, NewPull, Provider, PullRequest, RepoRef};
use crate::repo::{Repo, RepoError};
use crate::sync::{push_with_lease, PushOutcome, Token};
use crate::worktree::ThreadWorktree;

#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    #[error("checks failed: {} error(s)", .0.iter().filter(|f| f.level == checks::Level::Error).count())]
    Checks(Vec<checks::Finding>),
    #[error("nothing to submit: no changes against the default branch")]
    NoChanges,
    #[error("remote branch moved: {0}. Sync and try again.")]
    Lease(String),
    #[error(transparent)]
    Git(#[from] RepoError),
    #[error(transparent)]
    Provider(#[from] crate::provider::ProviderError),
}

/// Title and body proposed by an agent (#42) or built deterministically here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub title: String,
    pub summary: Option<String>,
    /// Condensed agent log, already rendered as markdown. None when no agent ran.
    pub agent_log: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    pub pull: PullRequest,
    pub created: bool,
    pub pushed_head: String,
    pub log_comment: Option<Comment>,
}

pub fn default_title(changes: &[FileChange], worktree: &Path) -> String {
    let docs: Vec<&FileChange> = changes
        .iter()
        .filter(|c| c.path.ends_with(".md") && c.path != index::AGENTS_FILE)
        .collect();
    match docs.len() {
        0 => "Update assets".to_string(),
        1 => {
            let title = std::fs::read_to_string(worktree.join(&docs[0].path))
                .ok()
                .and_then(|t| {
                    crate::frontmatter::parse(&t)
                        .ok()
                        .map(|(fm, body)| index::title_for(&docs[0].path, &fm, body))
                })
                .unwrap_or_else(|| docs[0].path.clone());
            format!("Update {title}")
        }
        n => format!("Update {n} documents"),
    }
}

pub fn build_body(
    summary: Option<&str>,
    changes: &[FileChange],
    base_branch: &str,
    repo_template: Option<&str>,
) -> String {
    let mut body = String::new();
    if let Some(s) = summary {
        body.push_str(s.trim());
        body.push_str("\n\n");
    }
    body.push_str("## Documents changed\n\n");
    for c in changes.iter().filter(|c| c.path != index::AGENTS_FILE) {
        let verb = match c.status {
            ChangeStatus::Added => "added",
            ChangeStatus::Modified => "changed",
            ChangeStatus::Deleted => "removed",
            ChangeStatus::Renamed => "renamed",
        };
        body.push_str(&format!("- `{}` {verb}\n", c.path));
    }
    body.push_str(&format!("\n_Opened with kmdn against `{base_branch}`._\n"));
    if let Some(t) = repo_template {
        body.push_str("\n---\n\n");
        body.push_str(t.trim());
        body.push('\n');
    }
    body
}

fn read_pr_template(root: &Path) -> Option<String> {
    [
        "PULL_REQUEST_TEMPLATE.md",
        ".github/PULL_REQUEST_TEMPLATE.md",
        ".github/pull_request_template.md",
        ".gitlab/merge_request_templates/Default.md",
        "docs/pull_request_template.md",
    ]
    .iter()
    .find_map(|p| std::fs::read_to_string(root.join(p)).ok())
}

const LOG_MARKER: &str = "<!-- kmdn:agent-log -->";

/// Submits `thread`. `remote_head` is the last known remote oid of the branch (None if never pushed).
#[allow(clippy::too_many_arguments)]
pub fn submit(
    repo: &Repo,
    thread: &ThreadWorktree,
    author: &Author,
    provider: &dyn Provider,
    repo_ref: &RepoRef,
    token: Option<&Token>,
    draft: &Draft,
    labels: &[String],
) -> Result<Submission, SubmitError> {
    let wt = &thread.path;
    let default = repo.default_branch()?;
    let base_ref = format!("refs/remotes/origin/{default}");

    // 1. Derived index, then a final commit of anything allowed and unsaved.
    let scan = index::scan_full(wt);
    index::write_agents_md_from(wt, &scan.docs)
        .map_err(|e| RepoError::Git(git2::Error::from_str(&e.to_string())))?;
    let allowed = allowed_set(DEFAULT_ALLOWED)
        .map_err(|e| RepoError::Git(git2::Error::from_str(&e.to_string())))?;
    commit_allowed(wt, "Update index", author, &allowed)?;

    // 2. Checks on the worktree.
    let findings = checks::run_with(wt, &checks::Options_::default_cap(), &scan);
    if checks::has_errors(&findings) {
        return Err(SubmitError::Checks(findings));
    }

    // 3. Changes against the base. Empty means nothing to review.
    let changes = thread_changes(wt, &base_ref)?;
    if changes.iter().all(|c| c.path == index::AGENTS_FILE) {
        return Err(SubmitError::NoChanges);
    }

    // 4. Push with lease.
    let wrepo = Repository::open(wt).map_err(RepoError::Git)?;
    let expected: Option<Oid> = wrepo
        .find_reference(&format!("refs/remotes/origin/{}", thread.branch))
        .ok()
        .and_then(|r| r.target());
    match push_with_lease(&wrepo, "origin", &thread.branch, expected, token)? {
        PushOutcome::Pushed => {}
        PushOutcome::LeaseFailed { remote_now } => return Err(SubmitError::Lease(remote_now)),
    }
    let pushed_head = wrepo
        .head()
        .map_err(RepoError::Git)?
        .target()
        .map(|o| o.to_string())
        .unwrap_or_default();

    // 5. Create or update the PR.
    let title = if draft.title.trim().is_empty() {
        default_title(&changes, wt)
    } else {
        draft.title.clone()
    };
    let body = build_body(
        draft.summary.as_deref(),
        &changes,
        &default,
        read_pr_template(repo.root()).as_deref(),
    );
    let existing = provider
        .list_open_pulls(repo_ref)?
        .into_iter()
        .find(|p| p.head_branch == thread.branch);
    let (pull, created) = match existing {
        Some(p) => (
            provider.update_pull(repo_ref, p.number, &title, &body)?,
            false,
        ),
        None => {
            let p = provider.create_pull(
                repo_ref,
                &NewPull {
                    title,
                    body,
                    head: thread.branch.clone(),
                    base: default.clone(),
                    draft: false,
                },
            )?;
            if !labels.is_empty() {
                // Labels ride on the issue side of a PR; best effort, never fatal.
                let _ = provider.create_issue_labels(repo_ref, p.number, labels);
            }
            (p, true)
        }
    };

    // 6. Condensed agent log as one comment, created once and updated afterwards (D58).
    let log_comment = match &draft.agent_log {
        None => None,
        Some(log) => {
            let text = format!("{LOG_MARKER}\n<details><summary>How this change was made</summary>\n\n{}\n\n</details>", log.trim());
            let prior = provider
                .list_comments(repo_ref, pull.number)?
                .into_iter()
                .find(|c| c.path.is_none() && c.body.starts_with(LOG_MARKER));
            Some(match prior {
                Some(c) => provider.update_comment(repo_ref, c.id, &text)?,
                None => provider.create_comment(repo_ref, pull.number, &text)?,
            })
        }
    };

    Ok(Submission {
        pull,
        created,
        pushed_head,
        log_comment,
    })
}

impl SubmitError {
    pub fn findings(&self) -> Option<&[checks::Finding]> {
        match self {
            SubmitError::Checks(f) => Some(f),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::mock::MockProvider;
    use crate::test_support::seeded_repo_with_origin;

    fn author() -> Author {
        Author {
            name: "A".into(),
            email: "a@x.io".into(),
        }
    }

    #[test]
    fn submits_creates_pr_then_updates_it() {
        let (_d, root, _up) = seeded_repo_with_origin();
        let repo = Repo::open(&root).unwrap();
        let wt = repo
            .create_thread_worktree("alice", "deploy", "refs/remotes/origin/main")
            .unwrap();
        std::fs::write(
            wt.path.join("deploy.md"),
            "---\ntitle: Deploy runbook\n---\n# Deploy\n",
        )
        .unwrap();
        let provider = MockProvider::default();
        let rr = RepoRef {
            owner: "acme".into(),
            name: "kb".into(),
        };

        let sub = submit(
            &repo,
            &wt,
            &author(),
            &provider,
            &rr,
            None,
            &Draft::default(),
            &["kmdn".into()],
        )
        .unwrap();
        assert!(sub.created);
        assert_eq!(sub.pull.title, "Update Deploy runbook");
        assert!(sub.pull.body.contains("- `deploy.md` added"));
        assert!(!sub.pull.body.contains("AGENTS.md"));
        assert_eq!(sub.pull.head_branch, "kmdn/alice/deploy");
        assert!(sub.log_comment.is_none());
        // remote has the branch with AGENTS.md committed
        let origin_branch = repo
            .git()
            .find_reference("refs/remotes/origin/kmdn/alice/deploy")
            .unwrap();
        assert!(origin_branch
            .peel_to_tree()
            .unwrap()
            .get_path(Path::new("AGENTS.md"))
            .is_ok());

        // second submit with an agent log: updates the PR, posts one comment
        std::fs::write(wt.path.join("rollback.md"), "# Rollback\n").unwrap();
        let draft = Draft {
            title: "Deploy and rollback".into(),
            summary: Some("Two runbooks.".into()),
            agent_log: Some("- user: write runbooks\n- agent: wrote 2 files".into()),
        };
        let sub2 = submit(&repo, &wt, &author(), &provider, &rr, None, &draft, &[]).unwrap();
        assert!(!sub2.created);
        assert_eq!(sub2.pull.number, sub.pull.number);
        assert_eq!(sub2.pull.title, "Deploy and rollback");
        assert!(sub2.pull.body.starts_with("Two runbooks."));
        let c = sub2.log_comment.unwrap();
        assert!(c.body.starts_with(LOG_MARKER));

        // third submit with nothing new: same PR, the log comment is updated, not duplicated
        let sub3 = submit(&repo, &wt, &author(), &provider, &rr, None, &draft, &[]).unwrap();
        assert!(!sub3.created);
        assert_eq!(sub3.log_comment.unwrap().id, c.id);
        assert_eq!(
            provider
                .comments
                .lock()
                .unwrap()
                .values()
                .flatten()
                .filter(|c| c.body.starts_with(LOG_MARKER))
                .count(),
            1
        );
    }

    #[test]
    fn refuses_on_check_errors_and_on_no_changes() {
        let (_d, root, _up) = seeded_repo_with_origin();
        let repo = Repo::open(&root).unwrap();
        let provider = MockProvider::default();
        let rr = RepoRef {
            owner: "acme".into(),
            name: "kb".into(),
        };

        let wt = repo
            .create_thread_worktree("a", "empty", "refs/remotes/origin/main")
            .unwrap();
        let err = submit(
            &repo,
            &wt,
            &author(),
            &provider,
            &rr,
            None,
            &Draft::default(),
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, SubmitError::NoChanges), "{err}");

        let wt2 = repo
            .create_thread_worktree("a", "broken", "refs/remotes/origin/main")
            .unwrap();
        std::fs::write(wt2.path.join("bad.md"), "See [gone](missing.md)\n").unwrap();
        let err = submit(
            &repo,
            &wt2,
            &author(),
            &provider,
            &rr,
            None,
            &Draft::default(),
            &[],
        )
        .unwrap_err();
        assert!(err.findings().is_some(), "{err}");
        assert!(provider.pulls.lock().unwrap().is_empty());
    }
}
