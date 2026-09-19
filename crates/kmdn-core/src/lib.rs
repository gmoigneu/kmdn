//! kmdn core library. No UI, no Tauri.
//!
//! Modules follow docs/03-architecture.md:
//! - `repo`: open a clone, detect provider from the remote, default branch
//! - `worktree`: one worktree per thread
//! - `index`: frontmatter, document tree, AGENTS.md generation
//! - `checks`: link, frontmatter, asset, stale-index checks
//!
//! Providers, sync, conflicts, and agent adapters land in later issues.

pub mod checks;
pub mod frontmatter;
pub mod index;
pub mod repo;
pub mod worktree;

pub use repo::{ProviderKind, RemoteInfo, Repo};
