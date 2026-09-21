//! Golden path against a real GitHub repository (01-vision.md, D2). Ignored by default.
//!
//! Run with:
//!   KMDN_E2E_GITHUB_TOKEN=ghp_... KMDN_E2E_GITHUB_REPO=owner/name \
//!   cargo test -p kmdn-core --test e2e_github -- --ignored --nocapture
//!
//! The repository is a throwaway. The test bootstraps it when empty, then: clone, start a
//! thread, edit by hand, submit for review, approve and merge as the reviewer, pull main,
//! and read the document back like an external agent would.

use std::path::Path;

use kmdn_core::bootstrap;
use kmdn_core::commit::{allowed_set, commit_allowed, Author, DEFAULT_ALLOWED};
use kmdn_core::index;
use kmdn_core::provider::github::GitHub;
use kmdn_core::provider::{MergeMethod, Provider, PullState, RepoRef, ReviewEvent, Side};
use kmdn_core::repo::Repo;
use kmdn_core::submit::{submit, Draft};
use kmdn_core::sync::{self, Token};

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

#[test]
#[ignore = "needs KMDN_E2E_GITHUB_TOKEN and KMDN_E2E_GITHUB_REPO"]
fn golden_path_github() {
    let (Some(token), Some(full)) = (env("KMDN_E2E_GITHUB_TOKEN"), env("KMDN_E2E_GITHUB_REPO"))
    else {
        eprintln!("skipping: env not set");
        return;
    };
    let (owner, name) = full.split_once('/').expect("owner/name");
    let repo_ref = RepoRef {
        owner: owner.into(),
        name: name.into(),
    };
    let gh = GitHub::new(&token);
    let cred = Token::github(&token);
    let me = gh.current_user().unwrap();
    let author = Author {
        name: me.login.clone(),
        email: me
            .email
            .clone()
            .unwrap_or_else(|| format!("{}@users.noreply.github.com", me.login)),
    };
    let url = format!("https://github.com/{full}.git");
    let tmp = tempfile::tempdir().unwrap();
    let clone = tmp.path().join("kb");

    // 0. Bootstrap the throwaway repo when it has no commits yet.
    match sync::clone_repo(&url, &clone, Some(&cred)) {
        Ok(r) if r.head().is_ok() => {}
        _ => {
            let _ = std::fs::remove_dir_all(&clone);
            let r = bootstrap::init_new_kb(
                &clone,
                "E2E KB",
                "Throwaway knowledge base for kmdn tests.",
                &author,
                Some(&url),
            )
            .unwrap();
            sync::push_with_lease(&r, "origin", "main", None, Some(&cred)).unwrap();
            eprintln!("bootstrapped {full}");
        }
    }
    let repo = Repo::open(&clone).unwrap();
    sync::fetch(repo.git(), "origin", Some(&cred)).unwrap();
    let default = repo.default_branch().unwrap();
    eprintln!("default branch {default}");

    // 1. Non-dev starts a thread and edits by hand.
    let run_id = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let slug = format!("e2e-{run_id}");
    let wt = repo
        .create_thread_worktree(&me.login, &slug, &format!("refs/remotes/origin/{default}"))
        .unwrap();
    let doc = format!("runbooks/e2e-{run_id}.md");
    std::fs::create_dir_all(wt.path.join("runbooks")).unwrap();
    std::fs::write(
        wt.path.join(&doc),
        format!("---\ntitle: E2E runbook {run_id}\ndescription: Written by the golden path test.\nstatus: published\n---\n# E2E runbook {run_id}\n\nStep one. See [getting started](../getting-started.md).\n"),
    )
    .unwrap();
    let allowed = allowed_set(DEFAULT_ALLOWED).unwrap();
    commit_allowed(&wt.path, "Add e2e runbook", &author, &allowed)
        .unwrap()
        .expect("a commit");

    // 2. Submit for review.
    let draft = Draft {
        title: String::new(),
        summary: Some("Automated golden path run.".into()),
        agent_log: Some(format!("- test: created {doc}")),
    };
    let sub = submit(
        &repo,
        &wt,
        &author,
        &gh,
        &repo_ref,
        Some(&cred),
        &draft,
        &["kmdn".into()],
    )
    .unwrap();
    assert!(sub.created, "PR should be new");
    assert_eq!(sub.pull.title, format!("Update E2E runbook {run_id}"));
    assert!(sub.log_comment.is_some());
    eprintln!("opened {}", sub.pull.url);

    // 3. Reviewer reads the PR, leaves an inline comment, approves (own PR: GitHub refuses, fall back to comment), merges.
    let files = gh.pull_files(&repo_ref, sub.pull.number).unwrap();
    assert!(
        files.contains(&doc) && files.contains(&index::AGENTS_FILE.to_string()),
        "{files:?}"
    );
    let c = gh
        .create_review_comment(
            &repo_ref,
            sub.pull.number,
            "Looks right.",
            &doc,
            7,
            Side::Right,
        )
        .unwrap();
    assert_eq!(c.path.as_deref(), Some(doc.as_str()));
    match gh.submit_review(&repo_ref, sub.pull.number, ReviewEvent::Approve, "LGTM") {
        Ok(()) => eprintln!("approved"),
        Err(e) => {
            eprintln!("approve refused (own PR): {e}");
            gh.submit_review(
                &repo_ref,
                sub.pull.number,
                ReviewEvent::Comment,
                "LGTM (self-review comment)",
            )
            .unwrap();
        }
    }
    let m = gh.mergeability(&repo_ref, sub.pull.number).unwrap();
    eprintln!("mergeability: {m:?}");
    // mergeable may be None while GitHub computes it; retry a few times.
    for _ in 0..10 {
        match gh.merge_pull(&repo_ref, sub.pull.number, MergeMethod::Squash) {
            Ok(()) => break,
            Err(e) => {
                eprintln!("merge not ready yet: {e}");
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
    }
    assert_eq!(
        gh.get_pull(&repo_ref, sub.pull.number).unwrap().state,
        PullState::Merged
    );

    // 4. Main clone pulls; the document is there; the index lists it. This is what an agent sees.
    sync::fetch(repo.git(), "origin", Some(&cred)).unwrap();
    let ff = sync::fast_forward_default(&repo).unwrap();
    eprintln!("fast-forward: {ff:?}");
    assert!(clone.join(&doc).exists(), "merged document missing on main");
    let agents = std::fs::read_to_string(clone.join(index::AGENTS_FILE)).unwrap();
    assert!(
        agents.contains(&doc),
        "AGENTS.md should list the new document"
    );
    assert!(
        kmdn_core::checks::run(&clone, &kmdn_core::checks::CheckOptions::default_cap()).is_empty(),
        "checks clean on main"
    );

    // 5. Thread is done: remove the worktree.
    repo.remove_thread_worktree(&slug, true).unwrap();
    assert!(!Path::new(&wt.path).exists());
}
