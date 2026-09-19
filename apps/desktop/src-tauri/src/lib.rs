//! Tauri shell. Commands are thin wrappers over kmdn-core (D59). Network and git work runs
//! on blocking threads so the UI never waits.

mod agents;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kmdn_core::bootstrap;
use kmdn_core::commit::{allowed_set, commit_allowed, Author, DEFAULT_ALLOWED};
use kmdn_core::diff::FileChange;
use kmdn_core::index::{self, Document, KbConfig};
use kmdn_core::local_changes::{self, LocalChanges};
use kmdn_core::provider::github::{self, GitHub};
use kmdn_core::provider::gitlab::GitLab;
use kmdn_core::provider::{
    self, Comment, DeviceCode, DevicePoll, MergeMethod, Mergeability, Provider, PullRequest,
    RepoRef, RepoSummary, ReviewEvent, Side, User,
};
use kmdn_core::repo::{ProviderKind, RemoteInfo, Repo};
use kmdn_core::secrets::{FileStore, SecretStore, StoredToken};
use kmdn_core::submit::{self, Draft, Submission};
use kmdn_core::sync::{self, FastForward, RebaseOutcome, Token};
use kmdn_core::worktree::ThreadWorktree;
use serde::Serialize;
use tauri::{Manager, State};

/// Public client id for the GitHub App device flow (D30). Empty means PAT only.
const GITHUB_CLIENT_ID: Option<&str> = option_env!("KMDN_GITHUB_CLIENT_ID");

pub struct AppState {
    secrets: Arc<FileStore>,
    data_dir: PathBuf,
    agents: Arc<agents::Runtime>,
}

#[derive(Serialize)]
pub struct KbInfo {
    pub root: PathBuf,
    pub remote: Option<RemoteInfo>,
    pub default_branch: String,
    pub head_branch: Option<String>,
    pub dirty_paths: Vec<String>,
    pub config: KbConfig,
    pub user: Author,
    /// A stored token exists for the remote's host.
    pub authenticated: bool,
}

#[derive(Serialize)]
pub struct SyncReport {
    pub main: FastForward,
    pub threads: Vec<(String, Result<RebaseOutcome, String>)>,
}

#[derive(Serialize)]
pub struct AuthStatus {
    pub hosts: Vec<StoredHost>,
    pub github_device_flow_available: bool,
}

#[derive(Serialize)]
pub struct StoredHost {
    pub host: String,
    pub login: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct SubmitOutcome {
    pub submission: Option<Submission>,
    pub findings: Vec<kmdn_core::checks::Finding>,
    pub error: Option<String>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

fn token_for(secrets: &FileStore, remote: &RemoteInfo) -> Option<(StoredToken, Token)> {
    let stored = secrets.get(&remote.host).ok().flatten()?;
    let token = match remote.provider {
        ProviderKind::GitHub => Token::github(&stored.token),
        _ => Token::gitlab(&stored.token),
    };
    Some((stored, token))
}

fn provider_for(
    secrets: &FileStore,
    remote: &RemoteInfo,
) -> Result<(Box<dyn Provider>, Token, RepoRef), String> {
    let (stored, token) =
        token_for(secrets, remote).ok_or_else(|| format!("not signed in to {}", remote.host))?;
    let provider: Box<dyn Provider> = match &remote.provider {
        ProviderKind::GitHub => Box::new(GitHub::new(&stored.token)),
        // Unknown hosts with a stored token are treated as self-hosted GitLab (D4).
        ProviderKind::GitLab { host } | ProviderKind::Unknown { host } => {
            Box::new(GitLab::new(host, &stored.token))
        }
    };
    Ok((
        provider,
        token,
        RepoRef {
            owner: remote.owner.clone(),
            name: remote.name.clone(),
        },
    ))
}

/// Identity for commits: the provider profile when signed in, else git config.
fn author_for(repo: &Repo, state: &AppState) -> Author {
    if let Ok(remote) = repo.remote_info("origin") {
        if let Some((stored, _)) = token_for(&state.secrets, &remote) {
            let email =
                std::fs::read_to_string(state.data_dir.join(format!("{}.email", remote.host)))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| format!("{}@users.noreply.github.com", stored.login));
            return Author {
                name: stored.login.clone(),
                email,
            };
        }
    }
    let cfg = repo.git().config().ok();
    let get = |k: &str| cfg.as_ref().and_then(|c| c.get_string(k).ok());
    Author {
        name: get("user.name").unwrap_or_else(|| "kmdn user".into()),
        email: get("user.email").unwrap_or_else(|| "kmdn@localhost".into()),
    }
}

fn base_ref(repo: &Repo) -> Result<String, String> {
    let default = repo.default_branch().map_err(err)?;
    let remote = format!("refs/remotes/origin/{default}");
    if repo.git().find_reference(&remote).is_ok() {
        Ok(remote)
    } else {
        Ok(format!("refs/heads/{default}"))
    }
}

fn worktree_for(repo: &Repo, slug: &str) -> Result<ThreadWorktree, String> {
    repo.list_thread_worktrees()
        .map_err(err)?
        .into_iter()
        .find(|t| t.slug == slug)
        .ok_or_else(|| format!("no thread {slug}"))
}

fn kb_info(state: &AppState, path: &str) -> Result<KbInfo, String> {
    let repo = Repo::open(path).map_err(err)?;
    let remote = repo.remote_info("origin").ok();
    let authenticated = remote
        .as_ref()
        .map(|r| token_for(&state.secrets, r).is_some())
        .unwrap_or(false);
    Ok(KbInfo {
        root: repo.root().to_path_buf(),
        default_branch: repo.default_branch().map_err(err)?,
        head_branch: repo.head_branch().map_err(err)?,
        dirty_paths: repo.dirty_paths().map_err(err)?,
        config: index::read_config(repo.root()),
        user: author_for(&repo, state),
        authenticated,
        remote,
    })
}

// ---------- knowledge base

#[tauri::command]
fn open_kb(state: State<AppState>, path: String) -> Result<KbInfo, String> {
    kb_info(&state, &path)
}

#[tauri::command]
async fn clone_kb(state: State<'_, AppState>, url: String, dest: String) -> Result<KbInfo, String> {
    let secrets = state.secrets.clone();
    let root = blocking(move || {
        let remote = RemoteInfo::parse(&url).map_err(err)?;
        let token = token_for(&secrets, &remote).map(|(_, t)| t);
        let dest = PathBuf::from(dest);
        if dest.exists()
            && dest
                .read_dir()
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        {
            return Err(format!(
                "{} already exists and is not empty",
                dest.display()
            ));
        }
        sync::clone_repo(&remote.https_url, &dest, token.as_ref()).map_err(err)?;
        Ok(dest)
    })
    .await?;
    kb_info(&state, &root.to_string_lossy())
}

#[tauri::command]
async fn create_kb(
    state: State<'_, AppState>,
    host: String,
    name: String,
    description: String,
    org: Option<String>,
    dest: String,
) -> Result<KbInfo, String> {
    let secrets = state.secrets.clone();
    let data_dir = state.data_dir.clone();
    let root = blocking(move || {
        let stored = secrets
            .get(&host)
            .map_err(err)?
            .ok_or_else(|| format!("not signed in to {host}"))?;
        let provider = provider_by_host(&host, &stored.token);
        let summary: RepoSummary = provider
            .create_repo(&name, &description, true, org.as_deref())
            .map_err(err)?;
        let email = std::fs::read_to_string(data_dir.join(format!("{host}.email")))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| format!("{}@users.noreply.{host}", stored.login));
        let author = Author {
            name: stored.login.clone(),
            email,
        };
        let dest = PathBuf::from(dest);
        let repo = bootstrap::init_new_kb(
            &dest,
            &name,
            &description,
            &author,
            Some(&summary.https_url),
        )
        .map_err(err)?;
        let token = if host == "github.com" {
            Token::github(&stored.token)
        } else {
            Token::gitlab(&stored.token)
        };
        sync::push_with_lease(&repo, "origin", "main", None, Some(&token)).map_err(err)?;
        Ok(dest)
    })
    .await?;
    kb_info(&state, &root.to_string_lossy())
}

#[tauri::command]
fn list_documents(root: String) -> Vec<Document> {
    index::scan(Path::new(&root))
}

#[tauri::command]
fn read_document(root: String, path: String) -> Result<String, String> {
    std::fs::read_to_string(Path::new(&root).join(&path)).map_err(err)
}

#[tauri::command]
fn run_checks(root: String) -> Vec<kmdn_core::checks::Finding> {
    kmdn_core::checks::run(
        Path::new(&root),
        &kmdn_core::checks::Options_::default_cap(),
    )
}

// ---------- threads

#[tauri::command]
fn list_threads(root: String) -> Result<Vec<ThreadWorktree>, String> {
    Repo::open(&root)
        .map_err(err)?
        .list_thread_worktrees()
        .map_err(err)
}

#[tauri::command]
fn create_thread(
    state: State<AppState>,
    root: String,
    slug: String,
) -> Result<ThreadWorktree, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let base = base_ref(&repo)?;
    let user = author_for(&repo, &state).name;
    repo.create_thread_worktree(&user, &slug, &base)
        .map_err(err)
}

#[tauri::command]
fn abandon_thread(root: String, slug: String) -> Result<(), String> {
    Repo::open(&root)
        .map_err(err)?
        .remove_thread_worktree(&slug, true)
        .map_err(err)
}

/// Writes the file inside the thread's worktree and commits it (D27).
#[tauri::command]
fn save_document(
    state: State<AppState>,
    root: String,
    slug: String,
    path: String,
    content: String,
) -> Result<Option<String>, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let wt = worktree_for(&repo, &slug)?;
    let full = wt.path.join(&path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    std::fs::write(&full, content).map_err(err)?;
    let title = Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or(path.clone());
    let allowed = allowed_set(DEFAULT_ALLOWED).map_err(err)?;
    let oid = commit_allowed(
        &wt.path,
        &format!("Update {title}"),
        &author_for(&repo, &state),
        &allowed,
    )
    .map_err(err)?;
    Ok(oid.map(|o| o.to_string()))
}

#[tauri::command]
fn thread_changes(root: String, slug: String) -> Result<Vec<FileChange>, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let wt = worktree_for(&repo, &slug)?;
    kmdn_core::diff::thread_changes(&wt.path, &base_ref(&repo)?).map_err(err)
}

#[tauri::command]
async fn submit_thread(
    state: State<'_, AppState>,
    root: String,
    slug: String,
    title: String,
    summary: Option<String>,
) -> Result<SubmitOutcome, String> {
    let secrets = state.secrets.clone();
    let data_dir = state.data_dir.clone();
    let agents_rt = state.agents.clone();
    let agent_log = state
        .agents
        .condensed_log(&slug)
        .filter(|l| !l.trim().is_empty());
    blocking(move || {
        let st = AppState {
            secrets,
            data_dir,
            agents: agents_rt,
        };
        let repo = Repo::open(&root).map_err(err)?;
        let wt = worktree_for(&repo, &slug)?;
        let remote = repo.remote_info("origin").map_err(err)?;
        let (provider, token, repo_ref) = provider_for(&st.secrets, &remote)?;
        let author = author_for(&repo, &st);
        let draft = Draft {
            title,
            summary,
            agent_log,
        };
        let labels = vec!["kmdn".to_string()];
        match submit::submit(
            &repo,
            &wt,
            &author,
            provider.as_ref(),
            &repo_ref,
            Some(&token),
            &draft,
            &labels,
        ) {
            Ok(s) => Ok(SubmitOutcome {
                submission: Some(s),
                findings: vec![],
                error: None,
            }),
            Err(e) => Ok(SubmitOutcome {
                findings: e.findings().map(|f| f.to_vec()).unwrap_or_default(),
                error: Some(e.to_string()),
                submission: None,
            }),
        }
    })
    .await
}

/// Fetch, fast-forward the main clone, rebase every thread (D21, D31).
#[tauri::command]
async fn sync_now(state: State<'_, AppState>, root: String) -> Result<SyncReport, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let Ok(remote) = repo.remote_info("origin") else {
            return Ok(SyncReport {
                main: FastForward::Skipped("no origin remote".into()),
                threads: vec![],
            });
        };
        let token = token_for(&secrets, &remote).map(|(_, t)| t);
        sync::fetch(repo.git(), "origin", token.as_ref()).map_err(err)?;
        let main = sync::fast_forward_default(&repo).map_err(err)?;
        let base = base_ref(&repo)?;
        let mut threads = Vec::new();
        for t in repo.list_thread_worktrees().map_err(err)? {
            match sync::rebase_worktree(&t.path, &base) {
                Ok(outcome) => threads.push((t.slug, Ok(outcome))),
                Err(e) => threads.push((t.slug, Err(e.to_string()))),
            }
        }
        Ok(SyncReport { main, threads })
    })
    .await
}

/// Registers a branch someone checked out in the main clone as a thread (D45). Never renames.
#[tauri::command]
fn adopt_branch(root: String, branch: String) -> Result<ThreadWorktree, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let default = repo.default_branch().map_err(err)?;
    // Put the main clone back on its default branch so the worktree can own the branch.
    if repo.head_branch().map_err(err)?.as_deref() == Some(branch.as_str()) {
        if repo.is_dirty().map_err(err)? {
            return Err(
                "the clone has uncommitted changes on that branch; commit or move them first"
                    .into(),
            );
        }
        repo.git()
            .set_head(&format!("refs/heads/{default}"))
            .map_err(err)?;
        repo.git()
            .checkout_head(Some(git2_checkout_force().as_mut()))
            .map_err(err)?;
    }
    local_changes::adopt_branch(&repo, &branch).map_err(err)
}

fn git2_checkout_force() -> Box<kmdn_core::git2::build::CheckoutBuilder<'static>> {
    let mut cb = kmdn_core::git2::build::CheckoutBuilder::new();
    cb.force();
    Box::new(cb)
}

/// Rebase one thread onto main, settling conflicts with the resolver's content per file (D21).
#[tauri::command]
async fn resolve_thread_conflicts(
    state: State<'_, AppState>,
    root: String,
    slug: String,
    resolutions: std::collections::HashMap<String, String>,
) -> Result<RebaseOutcome, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let wt = worktree_for(&repo, &slug)?;
        if let Ok(remote) = repo.remote_info("origin") {
            let token = token_for(&secrets, &remote).map(|(_, t)| t);
            sync::fetch(repo.git(), "origin", token.as_ref()).map_err(err)?;
        }
        let base = base_ref(&repo)?;
        sync::rebase_worktree_resolving(&wt.path, &base, &resolutions).map_err(err)
    })
    .await
}

/// Current conflict state of one thread against main, for the resolver (no changes made).
#[tauri::command]
async fn thread_conflicts(
    state: State<'_, AppState>,
    root: String,
    slug: String,
) -> Result<RebaseOutcome, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let wt = worktree_for(&repo, &slug)?;
        if let Ok(remote) = repo.remote_info("origin") {
            let token = token_for(&secrets, &remote).map(|(_, t)| t);
            sync::fetch(repo.git(), "origin", token.as_ref()).map_err(err)?;
        }
        let base = base_ref(&repo)?;
        sync::rebase_worktree(&wt.path, &base).map_err(err)
    })
    .await
}

#[tauri::command]
fn local_changes(root: String) -> Result<LocalChanges, String> {
    local_changes::inspect(&Repo::open(&root).map_err(err)?).map_err(err)
}

#[tauri::command]
fn move_local_changes_to_thread(
    state: State<AppState>,
    root: String,
    slug: String,
) -> Result<ThreadWorktree, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let user = author_for(&repo, &state).name;
    local_changes::move_to_new_thread(&repo, &user, &slug).map_err(err)
}

// ---------- discussions (D13): one provider issue per document path

#[derive(Serialize)]
pub struct Discussion {
    pub issue: Option<provider::Issue>,
    pub comments: Vec<Comment>,
}

#[tauri::command]
async fn doc_discussion(
    state: State<'_, AppState>,
    root: String,
    path: String,
) -> Result<Discussion, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let remote = repo.remote_info("origin").map_err(err)?;
        let (provider, _, repo_ref) = provider_for(&secrets, &remote)?;
        let issue = provider.find_issue(&repo_ref, "kmdn", &path).map_err(err)?;
        let comments = match &issue {
            Some(i) => provider
                .list_issue_comments(&repo_ref, i.number)
                .map_err(err)?,
            None => vec![],
        };
        Ok(Discussion { issue, comments })
    })
    .await
}

#[tauri::command]
async fn doc_discussion_comment(
    state: State<'_, AppState>,
    root: String,
    path: String,
    body: String,
) -> Result<Discussion, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let remote = repo.remote_info("origin").map_err(err)?;
        let (provider, _, repo_ref) = provider_for(&secrets, &remote)?;
        let issue = match provider.find_issue(&repo_ref, "kmdn", &path).map_err(err)? {
            Some(i) => i,
            None => provider
                .create_issue(&repo_ref, &path, &format!("Discussion about `{path}`, opened from kmdn. Comments here are about the published document, not a pending change."), &["kmdn"])
                .map_err(err)?,
        };
        provider.comment_issue(&repo_ref, issue.number, &body).map_err(err)?;
        let comments = provider.list_issue_comments(&repo_ref, issue.number).map_err(err)?;
        Ok(Discussion { issue: Some(issue), comments })
    })
    .await
}

// ---------- reviews

#[tauri::command]
async fn list_reviews(
    state: State<'_, AppState>,
    root: String,
) -> Result<Vec<PullRequest>, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let remote = repo.remote_info("origin").map_err(err)?;
        let (provider, _, repo_ref) = provider_for(&secrets, &remote)?;
        let prs = provider.list_open_pulls(&repo_ref).map_err(err)?;
        Ok(prs
            .into_iter()
            .filter(|p| provider::touches_markdown(&p.files))
            .collect())
    })
    .await
}

#[derive(Serialize)]
pub struct ReviewDetail {
    pub pull: PullRequest,
    pub changes: Vec<FileChange>,
    pub comments: Vec<Comment>,
    pub mergeability: Mergeability,
    pub head_sha: String,
}

/// Everything the review layout needs (D48): PR, rendered-diff inputs, comments, mergeability.
#[tauri::command]
async fn review_detail(
    state: State<'_, AppState>,
    root: String,
    number: u64,
) -> Result<ReviewDetail, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let remote = repo.remote_info("origin").map_err(err)?;
        let (provider, token, repo_ref) = provider_for(&secrets, &remote)?;
        let pull = provider.get_pull(&repo_ref, number).map_err(err)?;
        let head = kmdn_core::diff::fetch_branch(repo.git(), &pull.head_branch, Some(&token))
            .map_err(err)?;
        let base = kmdn_core::diff::fetch_branch(repo.git(), &pull.base_branch, Some(&token))
            .map_err(err)?;
        let changes = kmdn_core::diff::changes_between(repo.git(), base, head).map_err(err)?;
        let comments = provider.list_comments(&repo_ref, number).map_err(err)?;
        let mergeability = provider.mergeability(&repo_ref, number).map_err(err)?;
        Ok(ReviewDetail {
            pull,
            changes,
            comments,
            mergeability,
            head_sha: head.to_string(),
        })
    })
    .await
}

#[tauri::command]
async fn review_comment(
    state: State<'_, AppState>,
    root: String,
    number: u64,
    body: String,
    path: Option<String>,
    line: Option<u32>,
    side: Option<Side>,
) -> Result<Comment, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let remote = repo.remote_info("origin").map_err(err)?;
        let (provider, _, repo_ref) = provider_for(&secrets, &remote)?;
        match (path, line) {
            (Some(p), Some(l)) => provider
                .create_review_comment(&repo_ref, number, &body, &p, l, side.unwrap_or(Side::Right))
                .map_err(err),
            _ => provider
                .create_comment(&repo_ref, number, &body)
                .map_err(err),
        }
    })
    .await
}

#[tauri::command]
async fn review_submit(
    state: State<'_, AppState>,
    root: String,
    number: u64,
    event: ReviewEvent,
    body: String,
) -> Result<(), String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let remote = repo.remote_info("origin").map_err(err)?;
        let (provider, _, repo_ref) = provider_for(&secrets, &remote)?;
        provider
            .submit_review(&repo_ref, number, event, &body)
            .map_err(err)
    })
    .await
}

/// Merge (Publish). The provider enforces its own rules (D57); kmdn adds none.
#[tauri::command]
async fn review_merge(
    state: State<'_, AppState>,
    root: String,
    number: u64,
    method: Option<MergeMethod>,
) -> Result<(), String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let repo = Repo::open(&root).map_err(err)?;
        let remote = repo.remote_info("origin").map_err(err)?;
        let (provider, token, repo_ref) = provider_for(&secrets, &remote)?;
        provider
            .merge_pull(&repo_ref, number, method.unwrap_or(MergeMethod::Squash))
            .map_err(err)?;
        // Bring main forward right away so the merged document shows up.
        sync::fetch(repo.git(), "origin", Some(&token)).map_err(err)?;
        let _ = sync::fast_forward_default(&repo);
        Ok(())
    })
    .await
}

// ---------- agents

#[tauri::command]
async fn agent_detect() -> Vec<agents::DetectedAgent> {
    agents::detect().await
}

#[tauri::command]
async fn agent_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    root: String,
    slug: String,
    kind: kmdn_core::agents::AgentKind,
    mode: kmdn_core::agents::Mode,
    resume: Option<String>,
) -> Result<agents::SessionInfo, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let wt = worktree_for(&repo, &slug)?;
    state
        .agents
        .start(app, &state.data_dir, &slug, &wt.path, kind, mode, resume)
        .await
}

#[tauri::command]
async fn agent_send(state: State<'_, AppState>, slug: String, text: String) -> Result<(), String> {
    state.agents.send(&slug, &text).await
}

#[tauri::command]
async fn agent_reply_permission(
    state: State<'_, AppState>,
    slug: String,
    id: String,
    allow: bool,
    reason: Option<String>,
) -> Result<(), String> {
    state
        .agents
        .reply_permission(
            &slug,
            &id,
            allow,
            reason.as_deref().unwrap_or("denied by user"),
        )
        .await
}

#[tauri::command]
async fn agent_cancel(state: State<'_, AppState>, slug: String) -> Result<(), String> {
    state.agents.cancel(&slug).await
}

#[tauri::command]
async fn agent_stop(state: State<'_, AppState>, slug: String) -> Result<(), String> {
    state.agents.stop(&slug).await;
    Ok(())
}

#[tauri::command]
fn agent_session(state: State<AppState>, slug: String) -> Option<agents::SessionInfo> {
    state.agents.info(&slug)
}

// ---------- auth

#[tauri::command]
fn auth_status(state: State<AppState>) -> Result<AuthStatus, String> {
    let mut hosts = Vec::new();
    for h in state.secrets.hosts().map_err(err)? {
        if let Some(t) = state.secrets.get(&h).map_err(err)? {
            hosts.push(StoredHost {
                host: t.host,
                login: t.login,
                kind: t.kind,
            });
        }
    }
    Ok(AuthStatus {
        hosts,
        github_device_flow_available: GITHUB_CLIENT_ID.map(|c| !c.is_empty()).unwrap_or(false),
    })
}

#[tauri::command]
async fn auth_start_device_flow() -> Result<DeviceCode, String> {
    let client_id = GITHUB_CLIENT_ID
        .filter(|c| !c.is_empty())
        .ok_or("no GitHub client id compiled in; use a personal access token")?;
    blocking(move || {
        github::start_device_flow(
            client_id,
            "repo read:user user:email",
            github::DEVICE_CODE_URL,
        )
        .map_err(err)
    })
    .await
}

#[tauri::command]
async fn auth_poll_device_flow(
    state: State<'_, AppState>,
    device_code: String,
) -> Result<String, String> {
    let secrets = state.secrets.clone();
    let data_dir = state.data_dir.clone();
    let client_id = GITHUB_CLIENT_ID
        .filter(|c| !c.is_empty())
        .ok_or("no GitHub client id compiled in")?;
    blocking(move || {
        match github::poll_device_flow(client_id, &device_code, github::DEVICE_TOKEN_URL)
            .map_err(err)?
        {
            DevicePoll::Token(t) => {
                store_token(&secrets, &data_dir, "github.com", &t, "device_flow")
            }
            DevicePoll::Pending => Ok("pending".into()),
            DevicePoll::SlowDown => Ok("slow_down".into()),
            DevicePoll::Denied => Err("access denied".into()),
            DevicePoll::Expired => Err("code expired, start again".into()),
        }
    })
    .await
}

/// Validates a PAT against the host (github.com, gitlab.com, or a self-hosted GitLab) and stores it.
#[tauri::command]
async fn auth_save_pat(
    state: State<'_, AppState>,
    host: String,
    token: String,
) -> Result<User, String> {
    let secrets = state.secrets.clone();
    let data_dir = state.data_dir.clone();
    blocking(move || {
        let host = host
            .trim()
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_ascii_lowercase();
        if host.is_empty() || host.contains('/') {
            return Err("enter a hostname such as github.com or gitlab.example.org".into());
        }
        let login = store_token(&secrets, &data_dir, &host, &token, "pat")?;
        Ok(User {
            login,
            name: None,
            email: None,
            avatar_url: None,
        })
    })
    .await
}

fn provider_by_host(host: &str, token: &str) -> Box<dyn Provider> {
    if host == "github.com" {
        Box::new(GitHub::new(token))
    } else {
        Box::new(GitLab::new(host, token))
    }
}

/// Validates the token against the host, stores it, remembers the profile email.
fn store_token(
    secrets: &FileStore,
    data_dir: &Path,
    host: &str,
    token: &str,
    kind: &str,
) -> Result<String, String> {
    let user = provider_by_host(host, token).current_user().map_err(err)?;
    secrets
        .put(&StoredToken {
            host: host.into(),
            login: user.login.clone(),
            token: token.to_string(),
            kind: kind.into(),
        })
        .map_err(err)?;
    let email = user
        .email
        .clone()
        .unwrap_or_else(|| format!("{}@users.noreply.{host}", user.login));
    std::fs::write(data_dir.join(format!("{host}.email")), email).map_err(err)?;
    Ok(user.login)
}

#[tauri::command]
fn auth_sign_out(state: State<AppState>, host: String) -> Result<(), String> {
    state.secrets.delete(&host).map_err(err)
}

#[tauri::command]
async fn list_remote_repos(
    state: State<'_, AppState>,
    host: String,
) -> Result<Vec<RepoSummary>, String> {
    let secrets = state.secrets.clone();
    blocking(move || {
        let stored = secrets
            .get(&host)
            .map_err(err)?
            .ok_or_else(|| format!("not signed in to {host}"))?;
        provider_by_host(&host, &stored.token)
            .list_repos()
            .map_err(err)
    })
    .await
}

#[tauri::command]
fn default_clone_dir(name: String) -> String {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
        .join("kmdn")
        .join(name)
        .to_string_lossy()
        .to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from(".kmdn-data"));
            std::fs::create_dir_all(&dir).ok();
            app.manage(AppState {
                secrets: Arc::new(FileStore::new(dir.join("secrets.json"))),
                data_dir: dir,
                agents: Arc::new(agents::Runtime::default()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_kb,
            clone_kb,
            create_kb,
            list_documents,
            read_document,
            run_checks,
            list_threads,
            create_thread,
            abandon_thread,
            save_document,
            thread_changes,
            submit_thread,
            sync_now,
            local_changes,
            move_local_changes_to_thread,
            adopt_branch,
            resolve_thread_conflicts,
            thread_conflicts,
            doc_discussion,
            doc_discussion_comment,
            list_reviews,
            review_detail,
            review_comment,
            review_submit,
            review_merge,
            agent_detect,
            agent_start,
            agent_send,
            agent_reply_permission,
            agent_cancel,
            agent_stop,
            agent_session,
            auth_status,
            auth_start_device_flow,
            auth_poll_device_flow,
            auth_save_pat,
            auth_sign_out,
            list_remote_repos,
            default_clone_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running kmdn");
}
