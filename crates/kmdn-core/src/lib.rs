//! kmdn core library. No UI, no Tauri.
//!
//! Modules follow docs/03-architecture.md:
//! - `repo`: open a clone, detect provider from the remote, default branch
//! - `worktree`: one worktree per thread
//! - `index`: frontmatter, document tree, AGENTS.md generation
//! - `agents`: normalized agent events, policy, and one parser per agent (claude, codex, pi)
//! - `bootstrap`: new knowledge base from the template
//! - `checks`: link, frontmatter, asset, stale-index checks
//! - `commit`: commit on save, allowed paths only
//! - `diff`: a thread's changes versus the default branch, with old and new text
//! - `drafts`: unsaved editor text mirrored to local SQLite for crash recovery (D27, D12)
//! - `sync`: fetch, fast-forward, rebase worktrees, push with lease
//! - `local_changes`: the main clone's dirty state, move to thread, adopt branch
//! - `provider`: GitHub and GitLab behind one trait: PRs, reviews, comments, issues, auth
//! - `submit`: checks, index, push with lease, create or update the PR, agent log comment
//! - `secrets`: provider token storage
//!
//! Providers, the block-level conflict model, and agent adapters land in later issues.

pub mod agents;
pub mod bootstrap;
pub mod checks;
pub mod commit;
pub mod diff;
pub mod drafts;
pub mod frontmatter;
pub mod index;
pub mod local_changes;
pub mod provider;
pub mod repo;
pub mod secrets;
pub mod submit;
pub mod sync;
#[cfg(test)]
mod test_support;
pub mod worktree;

pub use git2;
pub use repo::{ProviderKind, RemoteInfo, Repo};
