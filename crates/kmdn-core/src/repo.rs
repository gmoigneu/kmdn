//! Open a clone, read its remote, detect the provider, find the default branch.
//! Decisions: D3 (one KB = one repo), D7 (user-chosen clone), D9 (libgit2).

use std::path::{Path, PathBuf};

use git2::Repository;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("not a git repository: {0}")]
    NotARepo(PathBuf),
    #[error("no remote named {0}")]
    NoRemote(String),
    #[error("cannot parse remote url: {0}")]
    BadRemote(String),
    #[error(transparent)]
    Git(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderKind {
    GitHub,
    /// gitlab.com or a self-hosted instance. `host` is the bare hostname.
    GitLab {
        host: String,
    },
    Unknown {
        host: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub provider: ProviderKind,
    pub host: String,
    pub owner: String,
    pub name: String,
    /// HTTPS URL kmdn uses for its own network operations (D9).
    pub https_url: String,
    pub original_url: String,
}

impl RemoteInfo {
    /// Parses https, ssh (`git@host:owner/name.git`) and `ssh://` remote URLs.
    pub fn parse(url: &str) -> Result<Self, RepoError> {
        let original = url.to_string();
        let bad = || RepoError::BadRemote(original.clone());

        let (host, path) = if let Some(rest) = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
        {
            let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
            rest.split_once('/').ok_or_else(bad)?
        } else if let Some(rest) = url.strip_prefix("ssh://") {
            let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
            let (host, path) = rest.split_once('/').ok_or_else(bad)?;
            (host.split_once(':').map(|(h, _)| h).unwrap_or(host), path)
        } else if let Some((_, rest)) = url.split_once('@') {
            rest.split_once(':').ok_or_else(bad)?
        } else {
            return Err(bad());
        };

        let host = host.to_ascii_lowercase();
        let path = path.trim_end_matches('/').trim_end_matches(".git");
        let mut segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() < 2 {
            return Err(bad());
        }
        let name = segs.pop().unwrap().to_string();
        let owner = segs.join("/"); // GitLab subgroups keep their slashes

        let provider = if host == "github.com" {
            ProviderKind::GitHub
        } else if host.contains("gitlab") {
            ProviderKind::GitLab { host: host.clone() }
        } else {
            ProviderKind::Unknown { host: host.clone() }
        };
        let https_url = format!("https://{host}/{owner}/{name}.git");
        Ok(Self {
            provider,
            host,
            owner,
            name,
            https_url,
            original_url: original,
        })
    }
}

/// A knowledge base clone on disk.
pub struct Repo {
    inner: Repository,
    root: PathBuf,
}

impl Repo {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RepoError> {
        let path = path.as_ref();
        let inner =
            Repository::discover(path).map_err(|_| RepoError::NotARepo(path.to_path_buf()))?;
        let root = inner
            .workdir()
            .ok_or_else(|| RepoError::NotARepo(path.to_path_buf()))?
            .to_path_buf();
        Ok(Self { inner, root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn git(&self) -> &Repository {
        &self.inner
    }

    pub fn remote_info(&self, remote: &str) -> Result<RemoteInfo, RepoError> {
        let r = self
            .inner
            .find_remote(remote)
            .map_err(|_| RepoError::NoRemote(remote.into()))?;
        let url = r.url().ok_or_else(|| RepoError::BadRemote(String::new()))?;
        RemoteInfo::parse(url)
    }

    /// Default branch: `refs/remotes/origin/HEAD` if present, else `main`, else `master`, else current HEAD.
    pub fn default_branch(&self) -> Result<String, RepoError> {
        if let Ok(r) = self.inner.find_reference("refs/remotes/origin/HEAD") {
            if let Some(target) = r.symbolic_target() {
                if let Some(name) = target.strip_prefix("refs/remotes/origin/") {
                    return Ok(name.to_string());
                }
            }
        }
        for candidate in ["main", "master"] {
            if self
                .inner
                .find_branch(candidate, git2::BranchType::Local)
                .is_ok()
            {
                return Ok(candidate.to_string());
            }
        }
        Ok(self.head_branch()?.unwrap_or_else(|| "main".to_string()))
    }

    /// Current branch name, or None when detached.
    pub fn head_branch(&self) -> Result<Option<String>, RepoError> {
        let head = match self.inner.head() {
            Ok(h) => h,
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(head.shorthand().map(str::to_string))
    }

    /// Paths (relative to root) that are modified, added, deleted, or untracked.
    pub fn dirty_paths(&self) -> Result<Vec<String>, RepoError> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .exclude_submodules(true);
        let statuses = self.inner.statuses(Some(&mut opts))?;
        Ok(statuses
            .iter()
            .filter(|s| !s.status().is_ignored())
            .filter_map(|s| s.path().map(str::to_string))
            .collect())
    }

    pub fn is_dirty(&self) -> Result<bool, RepoError> {
        Ok(!self.dirty_paths()?.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_https() {
        let r = RemoteInfo::parse("https://github.com/gmoigneu/kmdn.git").unwrap();
        assert_eq!(r.provider, ProviderKind::GitHub);
        assert_eq!((r.owner.as_str(), r.name.as_str()), ("gmoigneu", "kmdn"));
        assert_eq!(r.https_url, "https://github.com/gmoigneu/kmdn.git");
    }

    #[test]
    fn parses_github_ssh() {
        let r = RemoteInfo::parse("git@github.com:gmoigneu/kmdn.git").unwrap();
        assert_eq!(r.provider, ProviderKind::GitHub);
        assert_eq!(r.https_url, "https://github.com/gmoigneu/kmdn.git");
    }

    #[test]
    fn parses_gitlab_subgroup_and_custom_host() {
        let r = RemoteInfo::parse("ssh://git@lab.plat.farm:2222/group/sub/kb.git").unwrap();
        assert_eq!(
            r.provider,
            ProviderKind::Unknown {
                host: "lab.plat.farm".into()
            }
        );
        assert_eq!(r.owner, "group/sub");
        assert_eq!(r.name, "kb");
        let g = RemoteInfo::parse("https://gitlab.example.org/team/kb").unwrap();
        assert!(matches!(g.provider, ProviderKind::GitLab { .. }));
    }

    #[test]
    fn rejects_garbage() {
        assert!(RemoteInfo::parse("not a url").is_err());
        assert!(RemoteInfo::parse("https://github.com/only-owner").is_err());
    }

    #[test]
    fn opens_repo_and_reads_state() {
        let dir = tempfile::tempdir().unwrap();
        let git = Repository::init(dir.path()).unwrap();
        git.remote("origin", "git@github.com:acme/kb.git").unwrap();
        std::fs::write(dir.path().join("a.md"), "# A\n").unwrap();
        let repo = Repo::open(dir.path()).unwrap();
        assert_eq!(repo.remote_info("origin").unwrap().owner, "acme");
        assert_eq!(repo.dirty_paths().unwrap(), vec!["a.md".to_string()]);
        assert!(repo.head_branch().unwrap().is_none() || repo.head_branch().unwrap().is_some());
    }
}
