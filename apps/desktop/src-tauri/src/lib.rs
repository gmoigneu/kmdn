//! Tauri shell. Commands are thin wrappers over kmdn-core (D59).

use std::path::PathBuf;

use kmdn_core::index::{self, Document, KbConfig};
use kmdn_core::repo::{RemoteInfo, Repo};
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
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
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
    })
}

#[tauri::command]
fn list_documents(root: String) -> Vec<Document> {
    index::scan(std::path::Path::new(&root))
}

#[tauri::command]
fn read_document(root: String, path: String) -> Result<String, String> {
    let full = std::path::Path::new(&root).join(&path);
    std::fs::read_to_string(full).map_err(err)
}

#[tauri::command]
fn list_threads(root: String) -> Result<Vec<ThreadWorktree>, String> {
    Repo::open(&root)
        .map_err(err)?
        .list_thread_worktrees()
        .map_err(err)
}

#[tauri::command]
fn create_thread(root: String, user: String, slug: String) -> Result<ThreadWorktree, String> {
    let repo = Repo::open(&root).map_err(err)?;
    let base = format!("refs/heads/{}", repo.default_branch().map_err(err)?);
    repo.create_thread_worktree(&user, &slug, &base)
        .map_err(err)
}

#[tauri::command]
fn run_checks(root: String) -> Vec<kmdn_core::checks::Finding> {
    kmdn_core::checks::run(
        std::path::Path::new(&root),
        &kmdn_core::checks::Options_::default_cap(),
    )
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
            run_checks
        ])
        .run(tauri::generate_context!())
        .expect("error while running kmdn");
}
