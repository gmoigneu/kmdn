//! Provider abstraction (D4, D5, D13, D29). One trait, one implementation per host.
//! Blocking HTTP; callers run it off the UI thread.

pub mod github;
pub mod gitlab;
pub mod mock;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{status} from {url}: {body}")]
    Status {
        status: u16,
        url: String,
        body: String,
    },
    #[error("unexpected response: {0}")]
    Decode(String),
    #[error("not supported by this provider: {0}")]
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, ProviderError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub author: String,
    pub head_branch: String,
    pub base_branch: String,
    pub state: PullState,
    pub draft: bool,
    pub url: String,
    pub updated_at: String,
    /// Paths touched. Filled by `list_open_pulls` so the caller can keep markdown PRs only.
    pub files: Vec<String>,
    /// Logins whose review was requested, so the UI can list "requested from me" first.
    #[serde(default)]
    pub reviewers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPull {
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub draft: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mergeability {
    pub mergeable: Option<bool>,
    /// Provider's own word: clean, blocked, behind, dirty, unstable, unknown.
    pub state: String,
    pub approvals: u32,
    pub changes_requested: bool,
    pub checks_passing: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub url: String,
    /// Set for inline review comments.
    pub path: Option<String>,
    pub line: Option<u32>,
    pub side: Option<Side>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewEvent {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub url: String,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSummary {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub default_branch: String,
    pub https_url: String,
    pub description: Option<String>,
}

/// Marker comments used when a provider lacks a native approval API (D29).
pub const MARKER_APPROVED: &str = "<!-- kmdn:approved -->";
pub const MARKER_CHANGES_REQUESTED: &str = "<!-- kmdn:changes-requested -->";

pub trait Provider: Send + Sync {
    fn current_user(&self) -> Result<User>;

    fn list_repos(&self) -> Result<Vec<RepoSummary>>;
    fn create_repo(
        &self,
        name: &str,
        description: &str,
        private: bool,
        org: Option<&str>,
    ) -> Result<RepoSummary>;
    /// None when the provider cannot tell.
    fn default_branch_protected(&self, repo: &RepoRef, branch: &str) -> Result<Option<bool>>;

    fn list_open_pulls(&self, repo: &RepoRef) -> Result<Vec<PullRequest>>;
    fn get_pull(&self, repo: &RepoRef, number: u64) -> Result<PullRequest>;
    fn pull_files(&self, repo: &RepoRef, number: u64) -> Result<Vec<String>>;
    fn create_pull(&self, repo: &RepoRef, pull: &NewPull) -> Result<PullRequest>;
    fn update_pull(
        &self,
        repo: &RepoRef,
        number: u64,
        title: &str,
        body: &str,
    ) -> Result<PullRequest>;
    fn mergeability(&self, repo: &RepoRef, number: u64) -> Result<Mergeability>;
    fn merge_pull(&self, repo: &RepoRef, number: u64, method: MergeMethod) -> Result<()>;

    /// Conversation comments plus inline review comments, oldest first.
    fn list_comments(&self, repo: &RepoRef, number: u64) -> Result<Vec<Comment>>;
    fn create_comment(&self, repo: &RepoRef, number: u64, body: &str) -> Result<Comment>;
    fn update_comment(&self, repo: &RepoRef, comment_id: u64, body: &str) -> Result<Comment>;
    fn create_review_comment(
        &self,
        repo: &RepoRef,
        number: u64,
        body: &str,
        path: &str,
        line: u32,
        side: Side,
    ) -> Result<Comment>;
    fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        event: ReviewEvent,
        body: &str,
    ) -> Result<()>;

    fn find_issue(&self, repo: &RepoRef, label: &str, title: &str) -> Result<Option<Issue>>;
    fn create_issue(
        &self,
        repo: &RepoRef,
        title: &str,
        body: &str,
        labels: &[&str],
    ) -> Result<Issue>;
    fn list_issue_comments(&self, repo: &RepoRef, number: u64) -> Result<Vec<Comment>>;
    fn comment_issue(&self, repo: &RepoRef, number: u64, body: &str) -> Result<Comment>;
    /// Adds labels to a PR or issue. Best effort; providers without labels return Ok.
    fn create_issue_labels(&self, repo: &RepoRef, number: u64, labels: &[String]) -> Result<()>;
}

/// Keeps pull requests that touch at least one markdown document or asset (D22).
pub fn touches_markdown(files: &[String]) -> bool {
    files
        .iter()
        .any(|f| f.ends_with(".md") || f.split('/').any(|seg| seg == "assets"))
}

/// Device flow (D10). Returned by `start_device_flow` on the implementing module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevicePoll {
    Pending,
    SlowDown,
    Token(String),
    Denied,
    Expired,
}
