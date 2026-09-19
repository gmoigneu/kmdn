//! kmdn core library. No UI, no Tauri.
//!
//! Modules follow docs/03-architecture.md:
//! - `repo`: open a clone, detect provider from the remote, default branch
//! - `worktree`: one worktree per thread
//! - `index`: frontmatter, document tree, AGENTS.md generation
//! - `checks`: link, frontmatter, asset, stale-index checks
//! - `commit`: commit on save, allowed paths only
//! - `diff`: a thread's changes versus the default branch, with old and new text
//! - `sync`: fetch, fast-forward, rebase worktrees, push with lease
//! - `local_changes`: the main clone's dirty state, move to thread, adopt branch
//!
//! Providers, the block-level conflict model, and agent adapters land in later issues.

pub mod checks;
pub mod commit;
pub mod diff;
pub mod frontmatter;
pub mod index;
pub mod local_changes;
pub mod repo;
pub mod sync;
#[cfg(test)]
mod test_support;
pub mod worktree;

pub use repo::{ProviderKind, RemoteInfo, Repo};
