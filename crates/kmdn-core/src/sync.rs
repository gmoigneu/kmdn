//! Fetch, fast-forward the main clone, rebase thread worktrees, push (D21, D31, D56).
//! `AGENTS.md` is derived: a rebase conflict only on that file is resolved by regenerating it.

use std::path::Path;

use git2::{
    build::CheckoutBuilder, AnnotatedCommit, Cred, FetchOptions, Oid, PushOptions, RebaseOptions,
    RemoteCallbacks, Repository,
};
use serde::{Deserialize, Serialize};

use crate::index;
use crate::repo::{Repo, RepoError};

/// Credential used for kmdn's own HTTPS operations (D9). None for local or unauthenticated remotes.
#[derive(Debug, Clone)]
pub struct Token {
    pub username: String,
    pub secret: String,
}

impl Token {
    pub fn github(secret: &str) -> Self {
        Self {
            username: "x-access-token".into(),
            secret: secret.into(),
        }
    }
    pub fn gitlab(secret: &str) -> Self {
        Self {
            username: "oauth2".into(),
            secret: secret.into(),
        }
    }
}

pub(crate) fn callbacks(token: Option<&Token>) -> RemoteCallbacks<'_> {
    let mut cb = RemoteCallbacks::new();
    if let Some(t) = token {
        let t = t.clone();
        cb.credentials(move |_url, _user, _allowed| {
            Cred::userpass_plaintext(&t.username, &t.secret)
        });
    }
    cb
}

/// Clones `url` into `dest` using kmdn's own credentials (D7, D9).
pub fn clone_repo(url: &str, dest: &Path, token: Option<&Token>) -> Result<Repository, RepoError> {
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(callbacks(token));
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fo);
    Ok(builder.clone(url, dest)?)
}

/// The HTTPS URL kmdn should use for `remote` when its configured URL is SSH or git:// (D9).
/// None means the configured URL is already usable (http(s), file, or a local path).
pub fn https_url_for(repo: &Repository, remote: &str) -> Result<Option<String>, RepoError> {
    let r = repo.find_remote(remote)?;
    let url = r.url().unwrap_or("");
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("file://")
        || url.is_empty()
        || Path::new(url).exists()
    {
        return Ok(None);
    }
    match crate::repo::RemoteInfo::parse(url) {
        Ok(info) => Ok(Some(info.https_url)),
        Err(_) => Ok(None),
    }
}

pub fn fetch(repo: &Repository, remote: &str, token: Option<&Token>) -> Result<(), RepoError> {
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(callbacks(token));
    fo.prune(git2::FetchPrune::On);
    match https_url_for(repo, remote)? {
        None => {
            let mut r = repo.find_remote(remote)?;
            r.fetch::<&str>(&[], Some(&mut fo), None)?;
        }
        Some(https) => {
            // The user's SSH remote stays untouched; kmdn fetches over HTTPS with its token
            // into the same remote-tracking namespace.
            let mut anon = repo.remote_anonymous(&https)?;
            let spec = format!("+refs/heads/*:refs/remotes/{remote}/*");
            anon.fetch(&[spec.as_str()], Some(&mut fo), None)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FastForward {
    UpToDate,
    Forwarded {
        from: String,
        to: String,
    },
    /// Local default branch has commits the remote lacks, or the tree is dirty. Left alone.
    Skipped(String),
}

/// Fast-forwards the main clone's default branch to `origin/<default>` when clean and behind.
pub fn fast_forward_default(repo: &Repo) -> Result<FastForward, RepoError> {
    let git = repo.git();
    let default = repo.default_branch()?;
    if repo.head_branch()?.as_deref() != Some(default.as_str()) {
        return Ok(FastForward::Skipped(format!("HEAD is not on {default}")));
    }
    if repo.is_dirty()? {
        return Ok(FastForward::Skipped("working tree is dirty".into()));
    }
    let remote_ref = git.find_reference(&format!("refs/remotes/origin/{default}"))?;
    let remote_oid = remote_ref
        .target()
        .ok_or_else(|| git2::Error::from_str("remote ref has no target"))?;
    let local_oid = git
        .head()?
        .target()
        .ok_or_else(|| git2::Error::from_str("HEAD has no target"))?;
    if local_oid == remote_oid {
        return Ok(FastForward::UpToDate);
    }
    if !git.graph_descendant_of(remote_oid, local_oid)? {
        return Ok(FastForward::Skipped(
            "local branch has commits not on the remote".into(),
        ));
    }
    let mut r = git.find_reference(&format!("refs/heads/{default}"))?;
    r.set_target(remote_oid, "kmdn: fast-forward")?;
    git.set_head(&format!("refs/heads/{default}"))?;
    git.checkout_head(Some(CheckoutBuilder::new().force()))?;
    Ok(FastForward::Forwarded {
        from: local_oid.to_string(),
        to: remote_oid.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFile {
    pub path: String,
    /// Content on the default branch side.
    pub main: Option<String>,
    /// Content on the thread side.
    pub thread: Option<String>,
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebaseOutcome {
    UpToDate,
    Rebased {
        new_head: String,
    },
    /// Rebase was aborted and the worktree restored. The resolver gets these.
    Conflicts(Vec<ConflictFile>),
}

fn blob_text(repo: &Repository, entry: Option<&git2::IndexEntry>) -> Option<String> {
    let e = entry?;
    let blob = repo.find_blob(e.id).ok()?;
    Some(String::from_utf8_lossy(blob.content()).to_string())
}

/// Rebases the worktree's current branch onto `onto_ref` (e.g. `refs/remotes/origin/main`).
/// A conflict touching only `AGENTS.md` is resolved by regenerating it. Any other conflict
/// aborts the rebase and returns the conflicting files for the resolver.
pub fn rebase_worktree(worktree: &Path, onto_ref: &str) -> Result<RebaseOutcome, RepoError> {
    rebase_worktree_resolving(worktree, onto_ref, &std::collections::HashMap::new())
}

/// Like `rebase_worktree`, but a conflict on a path present in `resolutions` is settled with
/// that content (the resolver's output) and the rebase continues.
pub fn rebase_worktree_resolving(
    worktree: &Path,
    onto_ref: &str,
    resolutions: &std::collections::HashMap<String, String>,
) -> Result<RebaseOutcome, RepoError> {
    let repo = Repository::open(worktree)?;
    let head_ref = repo.head()?;
    let head_oid = head_ref
        .target()
        .ok_or_else(|| git2::Error::from_str("detached HEAD"))?;
    let onto = repo.find_reference(onto_ref)?;
    let onto_oid = onto
        .target()
        .ok_or_else(|| git2::Error::from_str("onto has no target"))?;

    if repo.graph_descendant_of(head_oid, onto_oid)? || head_oid == onto_oid {
        return Ok(RebaseOutcome::UpToDate);
    }
    let mut so = git2::StatusOptions::new();
    so.include_untracked(false);
    if !repo.statuses(Some(&mut so))?.is_empty() {
        return Err(git2::Error::from_str("worktree has uncommitted changes; save first").into());
    }

    let branch: AnnotatedCommit = repo.reference_to_annotated_commit(&head_ref)?;
    let upstream: AnnotatedCommit = repo.reference_to_annotated_commit(&onto)?;
    let mut opts = RebaseOptions::new();
    let mut rebase = repo.rebase(Some(&branch), Some(&upstream), None, Some(&mut opts))?;
    let sig = repo
        .signature()
        .unwrap_or_else(|_| git2::Signature::now("kmdn", "kmdn@localhost").unwrap());

    while let Some(op) = rebase.next() {
        op?;
        let mut idx = repo.index()?;
        if idx.has_conflicts() {
            let mut files = Vec::new();
            for c in idx.conflicts()? {
                let c = c?;
                let path = c
                    .our
                    .as_ref()
                    .or(c.their.as_ref())
                    .or(c.ancestor.as_ref())
                    .map(|e| String::from_utf8_lossy(&e.path).to_string())
                    .unwrap_or_default();
                // During a rebase "our" is the upstream (main) side and "their" is the replayed thread commit.
                files.push(ConflictFile {
                    path,
                    main: blob_text(&repo, c.our.as_ref()),
                    thread: blob_text(&repo, c.their.as_ref()),
                    base: blob_text(&repo, c.ancestor.as_ref()),
                });
            }
            let resolvable = files
                .iter()
                .all(|f| f.path == index::AGENTS_FILE || resolutions.contains_key(&f.path));
            if resolvable {
                let root = repo
                    .workdir()
                    .ok_or_else(|| git2::Error::from_str("no workdir"))?;
                for f in &files {
                    if let Some(content) = resolutions.get(&f.path) {
                        std::fs::write(root.join(&f.path), content)
                            .map_err(|e| git2::Error::from_str(&e.to_string()))?;
                        idx.remove_path(Path::new(&f.path))?;
                        idx.add_path(Path::new(&f.path))?;
                    }
                }
                if files.iter().any(|f| f.path == index::AGENTS_FILE) {
                    idx.remove_path(Path::new(index::AGENTS_FILE))?;
                    index::write_agents_md(root)
                        .map_err(|e| git2::Error::from_str(&e.to_string()))?;
                    idx.add_path(Path::new(index::AGENTS_FILE))?;
                }
                idx.write()?;
            } else if false {
                // Derived file: regenerate from the merged tree and continue (D56).
                let root = repo
                    .workdir()
                    .ok_or_else(|| git2::Error::from_str("no workdir"))?;
                idx.remove_path(Path::new(index::AGENTS_FILE))?;
                index::write_agents_md(root).map_err(|e| git2::Error::from_str(&e.to_string()))?;
                idx.add_path(Path::new(index::AGENTS_FILE))?;
                idx.write()?;
            } else {
                rebase.abort()?;
                return Ok(RebaseOutcome::Conflicts(files));
            }
        }
        rebase.commit(None, &sig, None)?;
    }
    rebase.finish(Some(&sig))?;
    let new_head = repo
        .head()?
        .target()
        .map(|o| o.to_string())
        .unwrap_or_default();
    Ok(RebaseOutcome::Rebased { new_head })
}

/// Deletes `branch` on the remote (05-git publish: "kmdn deletes the remote branch if the
/// provider did not"). Missing branches are not an error.
pub fn delete_remote_branch(
    repo: &Repository,
    remote: &str,
    branch: &str,
    token: Option<&Token>,
) -> Result<(), RepoError> {
    let mut po = PushOptions::new();
    po.remote_callbacks(callbacks(token));
    let refspec = format!(":refs/heads/{branch}");
    let result = match https_url_for(repo, remote)? {
        None => repo
            .find_remote(remote)?
            .push(&[refspec.as_str()], Some(&mut po)),
        Some(https) => repo
            .remote_anonymous(&https)?
            .push(&[refspec.as_str()], Some(&mut po)),
    };
    match result {
        Ok(()) => {}
        Err(e)
            if e.code() == git2::ErrorCode::NotFound
                || e.message().contains("not found")
                || e.message().contains("does not exist") => {}
        Err(e) => return Err(e.into()),
    }
    let _ = repo
        .find_reference(&format!("refs/remotes/{remote}/{branch}"))
        .and_then(|mut r| r.delete());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PushOutcome {
    Pushed,
    /// The remote branch moved since we last saw it. Nothing was pushed.
    LeaseFailed {
        remote_now: String,
    },
}

/// Force-pushes `branch` with a lease: refuses when the remote-tracking ref differs from
/// `expected_remote` (None means the branch must not exist remotely yet). Fetch first.
pub fn push_with_lease(
    repo: &Repository,
    remote: &str,
    branch: &str,
    expected_remote: Option<Oid>,
    token: Option<&Token>,
) -> Result<PushOutcome, RepoError> {
    fetch(repo, remote, token)?;
    let tracking = repo
        .find_reference(&format!("refs/remotes/{remote}/{branch}"))
        .ok()
        .and_then(|r| r.target());
    if tracking != expected_remote {
        return Ok(PushOutcome::LeaseFailed {
            remote_now: tracking
                .map(|o| o.to_string())
                .unwrap_or_else(|| "absent".into()),
        });
    }
    let mut po = PushOptions::new();
    po.remote_callbacks(callbacks(token));
    let refspec = format!("+refs/heads/{branch}:refs/heads/{branch}");
    match https_url_for(repo, remote)? {
        None => repo
            .find_remote(remote)?
            .push(&[refspec.as_str()], Some(&mut po))?,
        Some(https) => repo
            .remote_anonymous(&https)?
            .push(&[refspec.as_str()], Some(&mut po))?,
    }
    // libgit2 does not move the remote-tracking ref on push; mirror what git does.
    let local = repo
        .find_reference(&format!("refs/heads/{branch}"))?
        .target()
        .ok_or_else(|| git2::Error::from_str("branch has no target"))?;
    repo.reference(
        &format!("refs/remotes/{remote}/{branch}"),
        local,
        true,
        "kmdn: push",
    )?;
    Ok(PushOutcome::Pushed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::{allowed_set, commit_allowed, Author, DEFAULT_ALLOWED};
    use crate::test_support::{commit_on_origin, move_remote_branch, seeded_repo_with_origin};

    fn author() -> Author {
        Author {
            name: "A".into(),
            email: "a@x.io".into(),
        }
    }

    #[test]
    fn fast_forward_main_clone() {
        let (_d, root, upstream) = seeded_repo_with_origin();
        commit_on_origin(&upstream, "new.md", "# New\n", "add new");
        let repo = Repo::open(&root).unwrap();
        fetch(repo.git(), "origin", None).unwrap();
        let ff = fast_forward_default(&repo).unwrap();
        assert!(matches!(ff, FastForward::Forwarded { .. }), "{ff:?}");
        assert!(root.join("new.md").exists());
        assert_eq!(fast_forward_default(&repo).unwrap(), FastForward::UpToDate);
    }

    #[test]
    fn rebase_clean_and_conflicting_and_agents_md() {
        let (_d, root, upstream) = seeded_repo_with_origin();
        let repo = Repo::open(&root).unwrap();
        let allowed = allowed_set(DEFAULT_ALLOWED).unwrap();

        // Thread edits doc A; main edits doc B: clean rebase.
        let wt = repo
            .create_thread_worktree("a", "clean", "refs/remotes/origin/main")
            .unwrap();
        std::fs::write(wt.path.join("a.md"), "# A\nthread\n").unwrap();
        commit_allowed(&wt.path, "thread a", &author(), &allowed).unwrap();
        commit_on_origin(&upstream, "b.md", "# B\nmain\n", "main b");
        fetch(repo.git(), "origin", None).unwrap();
        let out = rebase_worktree(&wt.path, "refs/remotes/origin/main").unwrap();
        assert!(matches!(out, RebaseOutcome::Rebased { .. }), "{out:?}");
        assert!(wt.path.join("b.md").exists() && wt.path.join("a.md").exists());
        assert_eq!(
            rebase_worktree(&wt.path, "refs/remotes/origin/main").unwrap(),
            RebaseOutcome::UpToDate
        );

        // Both sides edit README.md: conflict returned, rebase aborted, worktree intact.
        let wt2 = repo
            .create_thread_worktree("a", "clash", "refs/remotes/origin/main")
            .unwrap();
        std::fs::write(wt2.path.join("README.md"), "# KB\nthread line\n").unwrap();
        commit_allowed(&wt2.path, "thread readme", &author(), &allowed).unwrap();
        commit_on_origin(&upstream, "README.md", "# KB\nmain line\n", "main readme");
        fetch(repo.git(), "origin", None).unwrap();
        let out = rebase_worktree(&wt2.path, "refs/remotes/origin/main").unwrap();
        match out {
            RebaseOutcome::Conflicts(files) => {
                assert_eq!(files.len(), 1);
                assert_eq!(files[0].path, "README.md");
                assert_eq!(files[0].main.as_deref(), Some("# KB\nmain line\n"));
                assert_eq!(files[0].thread.as_deref(), Some("# KB\nthread line\n"));
            }
            other => panic!("expected conflicts, got {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(wt2.path.join("README.md")).unwrap(),
            "# KB\nthread line\n"
        );
        assert!(Repository::open(&wt2.path).unwrap().state() == git2::RepositoryState::Clean);

        // Both sides regenerate AGENTS.md differently: resolved silently.
        let wt3 = repo
            .create_thread_worktree("a", "idx", "refs/remotes/origin/main")
            .unwrap();
        std::fs::write(wt3.path.join("c.md"), "---\ntitle: C\n---\n").unwrap();
        index::write_agents_md(&wt3.path).unwrap();
        commit_allowed(&wt3.path, "thread c", &author(), &allowed).unwrap();
        commit_on_origin(&upstream, "d.md", "---\ntitle: D\n---\n", "main d");
        // origin's AGENTS.md regenerated with d.md but without c.md
        index::write_agents_md(&upstream).unwrap();
        commit_on_origin(
            &upstream,
            index::AGENTS_FILE,
            &std::fs::read_to_string(upstream.join(index::AGENTS_FILE)).unwrap(),
            "main index",
        );
        fetch(repo.git(), "origin", None).unwrap();
        let out = rebase_worktree(&wt3.path, "refs/remotes/origin/main").unwrap();
        assert!(matches!(out, RebaseOutcome::Rebased { .. }), "{out:?}");
        assert!(
            !index::agents_md_is_stale(&wt3.path),
            "AGENTS.md should be regenerated from the merged tree"
        );
        let agents = std::fs::read_to_string(wt3.path.join(index::AGENTS_FILE)).unwrap();
        assert!(agents.contains("c.md") && agents.contains("d.md"));
    }

    #[test]
    fn push_with_lease_refuses_when_remote_moved() {
        let (_d, root, upstream) = seeded_repo_with_origin();
        let repo = Repo::open(&root).unwrap();
        let allowed = allowed_set(DEFAULT_ALLOWED).unwrap();
        let wt = repo
            .create_thread_worktree("a", "p", "refs/remotes/origin/main")
            .unwrap();
        std::fs::write(wt.path.join("p.md"), "# P\n").unwrap();
        commit_allowed(&wt.path, "p", &author(), &allowed).unwrap();
        let wrepo = Repository::open(&wt.path).unwrap();

        assert_eq!(
            push_with_lease(&wrepo, "origin", &wt.branch, None, None).unwrap(),
            PushOutcome::Pushed
        );
        let remote_oid = wrepo
            .find_reference(&format!("refs/remotes/origin/{}", wt.branch))
            .unwrap()
            .target()
            .unwrap();

        // someone else moves the remote branch
        move_remote_branch(&upstream, &wt.branch);

        std::fs::write(wt.path.join("p.md"), "# P2\n").unwrap();
        commit_allowed(&wt.path, "p2", &author(), &allowed).unwrap();
        let out = push_with_lease(&wrepo, "origin", &wt.branch, Some(remote_oid), None).unwrap();
        assert!(matches!(out, PushOutcome::LeaseFailed { .. }), "{out:?}");
        let now = wrepo
            .find_reference(&format!("refs/remotes/origin/{}", wt.branch))
            .unwrap()
            .target()
            .unwrap();
        assert_eq!(
            push_with_lease(&wrepo, "origin", &wt.branch, Some(now), None).unwrap(),
            PushOutcome::Pushed
        );
    }

    #[test]
    fn rebase_applies_resolutions_from_the_resolver() {
        let (_d, root, upstream) = seeded_repo_with_origin();
        let repo = Repo::open(&root).unwrap();
        let allowed = allowed_set(DEFAULT_ALLOWED).unwrap();
        let wt = repo
            .create_thread_worktree("a", "fix", "refs/remotes/origin/main")
            .unwrap();
        std::fs::write(wt.path.join("README.md"), "# KB\nthread line\n").unwrap();
        commit_allowed(&wt.path, "thread", &author(), &allowed).unwrap();
        commit_on_origin(&upstream, "README.md", "# KB\nmain line\n", "main");
        fetch(repo.git(), "origin", None).unwrap();
        assert!(matches!(
            rebase_worktree(&wt.path, "refs/remotes/origin/main").unwrap(),
            RebaseOutcome::Conflicts(_)
        ));
        let mut res = std::collections::HashMap::new();
        res.insert(
            "README.md".to_string(),
            "# KB\nmain line\nthread line\n".to_string(),
        );
        let out = rebase_worktree_resolving(&wt.path, "refs/remotes/origin/main", &res).unwrap();
        assert!(matches!(out, RebaseOutcome::Rebased { .. }), "{out:?}");
        assert_eq!(
            std::fs::read_to_string(wt.path.join("README.md")).unwrap(),
            "# KB\nmain line\nthread line\n"
        );
        assert_eq!(
            Repository::open(&wt.path).unwrap().state(),
            git2::RepositoryState::Clean
        );
    }

    #[test]
    fn ssh_and_git_remotes_are_rewritten_to_https_for_network_work() {
        let d = tempfile::tempdir().unwrap();
        let repo = Repository::init(d.path()).unwrap();
        repo.remote("origin", "git@github.com:acme/kb.git").unwrap();
        repo.remote("lab", "ssh://git@gitlab.example.org:2222/team/kb.git")
            .unwrap();
        repo.remote("web", "https://github.com/acme/kb.git")
            .unwrap();
        repo.remote("local", "file:///tmp/whatever.git").unwrap();
        assert_eq!(
            https_url_for(&repo, "origin").unwrap().as_deref(),
            Some("https://github.com/acme/kb.git")
        );
        assert_eq!(
            https_url_for(&repo, "lab").unwrap().as_deref(),
            Some("https://gitlab.example.org/team/kb.git")
        );
        assert_eq!(https_url_for(&repo, "web").unwrap(), None);
        assert_eq!(https_url_for(&repo, "local").unwrap(), None);
    }
}
