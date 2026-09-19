//! Tauri shell. Commands are thin wrappers over kmdn-core (D59).

use std::path::{Path, PathBuf};

use kmdn_core::commit::{allowed_set, commit_allowed, Author, DEFAULT_ALLOWED};
use kmdn_core::diff::FileChange;
use kmdn_core::index::{self, Document, KbConfig};
use kmdn_core::local_changes::{self, LocalChanges};
use kmdn_core::repo::{RemoteInfo, Repo};
use kmdn_core::sync::{self, FastForward, RebaseOutcome};
use kmdn_core::worktree::ThreadWorktree;
use serde::Serialize;

#[derive(Serialize)]
pub struct KbInfo {
    pub root: PathBuf,
    pub remote: Option<RemoteInfo>,
    pub default_branch: String,
    pub head_branch: Option<String>,
    pub dirty_paths: Vec<String>,
    pub config: KbConfig,
    pub user: Author,
}

#[derive(Serialize)]
pub struct SyncReport {
    pub main: FastForward,
    pub threads: Vec<(String, Result<RebaseOutcome, String>)>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Identity for commits. Provider profile lands with auth (#14); until then git config.
fn author_for(repo: &Repo) -> Author {
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

#[tauri::command]
fn open_kb(path: String) -> Result<KbInfo, String> {
    let repo = Repo::open(&path).map_err(err)?;
    Ok(KbInfo {
        root: repo.root().to_path_buf(),
        remote: repo.remote_info("origin").ok(),
        default_branch: repo.default_branch().map_err(err)?,
        head_branch: repo.head_branch().map_err(err)?,
        dirty_paths: repo.dirty_paths().map_err(err)?,
        config: index::read_config(repo.root()),
        user: author_for(&repo),
    })
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
fn list_threads(root: String) -> Result<Vec<ThreadWorktree>, String> {
    Repo::open(&root)
        .map_err(err)?
        .list_thread_worktrees()
        .map_err(err)
}

#[tauri::command]
fn create_thread(root: String, slug: String) -> Result<ThreadWorktree, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let base = base_ref(&repo)?;
    let user = author_for(&repo).name;
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
        &author_for(&repo),
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
fn run_checks(root: String) -> Vec<kmdn_core::checks::Finding> {
    kmdn_core::checks::run(
        Path::new(&root),
        &kmdn_core::checks::Options_::default_cap(),
    )
}

/// Fetch, fast-forward the main clone, rebase every thread (D21, D31). Token wiring lands with #14.
#[tauri::command]
fn sync_now(root: String) -> Result<SyncReport, String> {
    let repo = Repo::open(&root).map_err(err)?;
    if repo.git().find_remote("origin").is_err() {
        return Ok(SyncReport {
            main: FastForward::Skipped("no origin remote".into()),
            threads: vec![],
        });
    }
    sync::fetch(repo.git(), "origin", None).map_err(err)?;
    let main = sync::fast_forward_default(&repo).map_err(err)?;
    let base = base_ref(&repo)?;
    let mut threads = Vec::new();
    for t in repo.list_thread_worktrees().map_err(err)? {
        match sync::rebase_worktree(&t.path, &base) {
            Ok(outcome) => threads.push((t.slug, Ok(outcome))),
            // Typically "worktree has uncommitted changes"; the thread keeps its state.
            Err(e) => threads.push((t.slug, Err(e.to_string()))),
        }
    }
    Ok(SyncReport { main, threads })
}

#[tauri::command]
fn local_changes(root: String) -> Result<LocalChanges, String> {
    local_changes::inspect(&Repo::open(&root).map_err(err)?).map_err(err)
}

#[tauri::command]
fn move_local_changes_to_thread(root: String, slug: String) -> Result<ThreadWorktree, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let user = author_for(&repo).name;
    local_changes::move_to_new_thread(&repo, &user, &slug).map_err(err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_kb,
            list_documents,
            read_document,
            list_threads,
            create_thread,
            abandon_thread,
            save_document,
            thread_changes,
            run_checks,
            sync_now,
            local_changes,
            move_local_changes_to_thread
        ])
        .run(tauri::generate_context!())
        .expect("error while running kmdn");
}
