//! Golden path against a real GitLab project (gitlab.com or self-hosted). Ignored by default.
//!
//! Run with:
//!   KMDN_E2E_GITLAB_TOKEN=glpat-... KMDN_E2E_GITLAB_REPO=gitlab.com/group/name \
//!   cargo test -p kmdn-core --test e2e_gitlab -- --ignored --nocapture
//!
//! Same steps as the GitHub test: bootstrap when empty, clone, thread, edit, submit, inline
//! comment, approve (own MR may be refused by the instance), merge, pull main, read back.

use std::path::Path;

use kmdn_core::bootstrap;
use kmdn_core::commit::{allowed_set, commit_allowed, Author, DEFAULT_ALLOWED};
use kmdn_core::index;
use kmdn_core::provider::gitlab::GitLab;
use kmdn_core::provider::{MergeMethod, Provider, PullState, RepoRef, ReviewEvent, Side};
use kmdn_core::repo::Repo;
use kmdn_core::submit::{submit, Draft};
use kmdn_core::sync::{self, Token};

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

#[test]
#[ignore = "needs KMDN_E2E_GITLAB_TOKEN and KMDN_E2E_GITLAB_REPO (host/group/name)"]
fn golden_path_gitlab() {
    let (Some(token), Some(full)) = (env("KMDN_E2E_GITLAB_TOKEN"), env("KMDN_E2E_GITLAB_REPO"))
    else {
        eprintln!("skipping: env not set");
        return;
    };
    let (host, path) = full.split_once('/').expect("host/group/name");
    let (owner, name) = path.rsplit_once('/').expect("group/name");
    let repo_ref = RepoRef {
        owner: owner.into(),
        name: name.into(),
    };
    let gl = GitLab::new(host, &token);
    let cred = Token::gitlab(&token);
    let me = gl.current_user().unwrap();
    let author = Author {
        name: me.name.clone().unwrap_or_else(|| me.login.clone()),
        email: me
            .email
            .clone()
            .unwrap_or_else(|| format!("{}@users.noreply.{host}", me.login)),
    };
    let url = format!("https://{host}/{owner}/{name}.git");
    let tmp = tempfile::tempdir().unwrap();
    let clone = tmp.path().join("kb");

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

    let draft = Draft {
        title: String::new(),
        summary: Some("Automated golden path run.".into()),
        agent_log: Some(format!("- test: created {doc}")),
    };
    let sub = submit(
        &repo,
        &wt,
        &author,
        &gl,
        &repo_ref,
        Some(&cred),
        &draft,
        &["kmdn".into()],
    )
    .unwrap();
    assert!(sub.created);
    eprintln!("opened {}", sub.pull.url);

    let files = gl.pull_files(&repo_ref, sub.pull.number).unwrap();
    assert!(files.contains(&doc), "{files:?}");
    let c = gl
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
    match gl.submit_review(&repo_ref, sub.pull.number, ReviewEvent::Approve, "LGTM") {
        Ok(()) => eprintln!("approved (natively or by marker)"),
        Err(e) => eprintln!("approve refused: {e}"),
    }
    let m = gl.mergeability(&repo_ref, sub.pull.number).unwrap();
    eprintln!("mergeability: {m:?}");
    for _ in 0..12 {
        match gl.merge_pull(&repo_ref, sub.pull.number, MergeMethod::Squash) {
            Ok(()) => break,
            Err(e) => {
                eprintln!("merge not ready yet: {e}");
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
    }
    assert_eq!(
        gl.get_pull(&repo_ref, sub.pull.number).unwrap().state,
        PullState::Merged
    );

    sync::fetch(repo.git(), "origin", Some(&cred)).unwrap();
    let ff = sync::fast_forward_default(&repo).unwrap();
    eprintln!("fast-forward: {ff:?}");
    assert!(clone.join(&doc).exists());
    let agents = std::fs::read_to_string(clone.join(index::AGENTS_FILE)).unwrap();
    assert!(agents.contains(&doc));
    assert!(kmdn_core::checks::run(&clone, &kmdn_core::checks::Options_::default_cap()).is_empty());
    repo.remove_thread_worktree(&slug, true).unwrap();
    assert!(!Path::new(&wt.path).exists());
}
