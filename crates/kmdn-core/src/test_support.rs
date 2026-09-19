//! Shared fixtures for tests: seeded repos, optional local bare origin.
#![cfg(test)]

use std::path::{Path, PathBuf};

use git2::{Repository, Signature};

fn sig() -> Signature<'static> {
    Signature::now("t", "t@example.com").unwrap()
}

/// Commits `content` at `rel` in the repo at `root` (a non-bare working repo). Returns the oid.
pub fn commit_file(root: &Path, rel: &str, content: &str, message: &str) -> git2::Oid {
    let repo = Repository::open(root).unwrap();
    let full = root.join(rel);
    if let Some(p) = full.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(&full, content).unwrap();
    let mut idx = repo.index().unwrap();
    idx.add_path(Path::new(rel)).unwrap();
    idx.write().unwrap();
    let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    let s = sig();
    repo.commit(Some("HEAD"), &s, &s, message, &tree, &parents)
        .unwrap()
}

/// A repo at `<tmp>/kb` on `main` with one README commit.
pub fn seeded_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("kb");
    std::fs::create_dir(&root).unwrap();
    Repository::init_opts(
        &root,
        git2::RepositoryInitOptions::new().initial_head("main"),
    )
    .unwrap();
    commit_file(&root, "README.md", "# KB\n", "init");
    (dir, root)
}

/// Like `seeded_repo` but cloned from a local BARE origin at `<tmp>/origin.git`. A second
/// working clone at `<tmp>/upstream` stands in for "someone else": commit there and call
/// `push_upstream` to move the origin. Returns (tmp, clone root, upstream root).
pub fn seeded_repo_with_origin() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin.git");
    Repository::init_opts(
        &origin,
        git2::RepositoryInitOptions::new()
            .bare(true)
            .initial_head("main"),
    )
    .unwrap();
    let url = format!("file://{}", origin.display());

    let upstream = dir.path().join("upstream");
    std::fs::create_dir(&upstream).unwrap();
    Repository::init_opts(
        &upstream,
        git2::RepositoryInitOptions::new().initial_head("main"),
    )
    .unwrap();
    Repository::open(&upstream)
        .unwrap()
        .remote("origin", &url)
        .unwrap();
    commit_file(&upstream, "README.md", "# KB\n", "init");
    push_upstream(&upstream, "main");

    let root = dir.path().join("kb");
    Repository::clone(&url, &root).unwrap();
    (dir, root, upstream)
}

/// Pushes `branch` from the upstream working clone to the bare origin (force).
pub fn push_upstream(upstream: &Path, branch: &str) {
    let repo = Repository::open(upstream).unwrap();
    let mut r = repo.find_remote("origin").unwrap();
    let spec = format!("+refs/heads/{branch}:refs/heads/{branch}");
    r.push(&[spec.as_str()], None).unwrap();
}

/// Commits on the upstream clone's current branch and pushes it to the origin.
pub fn commit_on_origin(upstream: &Path, rel: &str, content: &str, message: &str) -> git2::Oid {
    let oid = commit_file(upstream, rel, content, message);
    let branch = Repository::open(upstream)
        .unwrap()
        .head()
        .unwrap()
        .shorthand()
        .unwrap()
        .to_string();
    push_upstream(upstream, &branch);
    oid
}

/// Simulates another client adding a commit to `branch` on the origin.
pub fn move_remote_branch(upstream: &Path, branch: &str) {
    let repo = Repository::open(upstream).unwrap();
    let mut r = repo.find_remote("origin").unwrap();
    let spec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    r.fetch(&[spec.as_str()], None, None).unwrap();
    let head = repo
        .find_reference(&format!("refs/remotes/origin/{branch}"))
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let s = sig();
    let tree = head.tree().unwrap();
    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &s,
        &s,
        "moved by someone else",
        &tree,
        &[&head],
    )
    .unwrap();
    push_upstream(upstream, branch);
}
