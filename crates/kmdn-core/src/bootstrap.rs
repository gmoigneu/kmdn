//! New knowledge base from the template (D61): files, first commit, remote.

use std::path::Path;

use git2::{IndexAddOption, Repository, Signature};

use crate::commit::Author;
use crate::index;
use crate::repo::{ProviderKind, RepoError};

/// Optional CI job that runs `kmdn-cli check` on pull requests (D60). Opt-in, never automatic.
pub const GITHUB_CI_TEMPLATE: &str = include_str!("../../../templates/ci/github-kmdn-check.yml");
pub const GITLAB_CI_TEMPLATE: &str = include_str!("../../../templates/ci/gitlab-kmdn-check.yml");

/// Where the CI check lives for a provider, relative to the repo root.
pub fn ci_check_path(provider: &ProviderKind) -> &'static str {
    match provider {
        ProviderKind::GitHub => ".github/workflows/kmdn-check.yml",
        _ => ".gitlab-ci.yml",
    }
}

/// Writes the CI check for `provider` into `root`. Refuses to overwrite an existing file so a
/// hand-maintained pipeline is never clobbered.
pub fn write_ci_check(root: &Path, provider: &ProviderKind) -> std::io::Result<std::path::PathBuf> {
    let rel = ci_check_path(provider);
    let full = root.join(rel);
    if full.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{rel} already exists; add the kmdn-check job to it by hand"),
        ));
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = match provider {
        ProviderKind::GitHub => GITHUB_CI_TEMPLATE,
        _ => GITLAB_CI_TEMPLATE,
    };
    std::fs::write(&full, body)?;
    Ok(full)
}

pub fn write_template(dest: &Path, name: &str, description: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dest.join(index::CONFIG_DIR))?;
    std::fs::write(
        dest.join(index::CONFIG_DIR).join("config.yaml"),
        format!(
            "version: 1\nname: {name}\ndescription: {description}\nbranch_prefix: kmdn/\nreview:\n  labels: [kmdn]\n  post_agent_log: true\nassets:\n  max_bytes: 5242880\nagents:\n  allowed_paths: [\"**/*.md\", \"**/assets/**\"]\n"
        ),
    )?;
    std::fs::write(
        dest.join("README.md"),
        format!("# {name}\n\n{description}\n\nStart here. This knowledge base is maintained with kmdn.\n"),
    )?;
    std::fs::write(
        dest.join("getting-started.md"),
        "---\ntitle: Getting started\ndescription: How this knowledge base is organized and how to contribute.\nstatus: published\norder: 1\n---\n# Getting started\n\nDocuments are markdown files. Folders are sections. Every change is a pull request.\n",
    )?;
    index::write_agents_md(dest)?;
    Ok(())
}

/// Initializes a repo on `main` at `dest` with the template committed and `origin` set.
pub fn init_new_kb(
    dest: &Path,
    name: &str,
    description: &str,
    author: &Author,
    origin_url: Option<&str>,
) -> Result<Repository, RepoError> {
    std::fs::create_dir_all(dest).map_err(|e| git2::Error::from_str(&e.to_string()))?;
    let repo = Repository::init_opts(
        dest,
        git2::RepositoryInitOptions::new().initial_head("main"),
    )?;
    write_template(dest, name, description).map_err(|e| git2::Error::from_str(&e.to_string()))?;
    let mut idx = repo.index()?;
    idx.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    idx.write()?;
    let tree_oid = idx.write_tree()?;
    {
        let tree = repo.find_tree(tree_oid)?;
        let sig = Signature::now(&author.name, &author.email)?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Initialize knowledge base",
            &tree,
            &[],
        )?;
    }
    if let Some(url) = origin_url {
        repo.remote("origin", url)?;
    }
    Ok(repo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_valid_kb() {
        let d = tempfile::tempdir().unwrap();
        let dest = d.path().join("kb");
        let author = Author {
            name: "A".into(),
            email: "a@x.io".into(),
        };
        let repo = init_new_kb(
            &dest,
            "Team KB",
            "Docs.",
            &author,
            Some("https://github.com/acme/kb.git"),
        )
        .unwrap();
        assert_eq!(repo.head().unwrap().shorthand(), Some("main"));
        assert!(dest.join("AGENTS.md").exists() && dest.join(".kmdn/config.yaml").exists());
        assert!(crate::checks::run(&dest, &crate::checks::CheckOptions::default_cap()).is_empty());
        assert_eq!(
            crate::index::read_config(&dest).name.as_deref(),
            Some("Team KB")
        );
        assert_eq!(
            repo.find_remote("origin").unwrap().url(),
            Some("https://github.com/acme/kb.git")
        );
    }

    #[test]
    fn writes_the_ci_check_once() {
        let d = tempfile::tempdir().unwrap();
        let p = write_ci_check(d.path(), &ProviderKind::GitHub).unwrap();
        assert!(p.ends_with(".github/workflows/kmdn-check.yml"));
        assert!(std::fs::read_to_string(&p)
            .unwrap()
            .contains("kmdn-cli check"));
        assert!(write_ci_check(d.path(), &ProviderKind::GitHub).is_err());
        let g = write_ci_check(
            d.path(),
            &ProviderKind::GitLab {
                host: "gitlab.com".into(),
            },
        )
        .unwrap();
        assert!(g.ends_with(".gitlab-ci.yml"));
    }
}
