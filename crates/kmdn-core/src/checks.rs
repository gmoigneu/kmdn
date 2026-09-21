//! Checks shared by the submit flow and `kmdn-cli check` (D60):
//! broken relative links, invalid frontmatter, oversize assets, stale AGENTS.md.

use std::path::{Component, Path, PathBuf};

use pulldown_cmark::{Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};

use crate::index;

pub const DEFAULT_ASSET_CAP: u64 = 5 * 1024 * 1024;
const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, rename = "FindingLevel"))]
pub enum Level {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, rename = "FindingKind"))]
pub enum Kind {
    BrokenLink,
    InvalidFrontmatter,
    OversizeAsset,
    StaleIndex,
    NonImageAsset,
    /// A directory that agent CLIs read configuration or hooks from.
    AgentConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Finding {
    pub level: Level,
    pub kind: Kind,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    pub asset_cap: u64,
}

impl CheckOptions {
    pub fn default_cap() -> Self {
        Self {
            asset_cap: DEFAULT_ASSET_CAP,
        }
    }
}

/// Resolves a relative link against the linking document. Returns None for
/// external, anchor-only, or escaping links.
fn resolve_link(root: &Path, doc_rel: &str, target: &str) -> Option<PathBuf> {
    let target = target.split(['#', '?']).next().unwrap_or("");
    if target.is_empty()
        || target.contains("://")
        || target.starts_with("mailto:")
        || target.starts_with('/')
    {
        return None;
    }
    let base = Path::new(doc_rel).parent().unwrap_or(Path::new(""));
    let mut out = PathBuf::new();
    for c in base.join(target).components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    return None; // escapes the repo
                }
            }
            Component::Normal(n) => out.push(n),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(root.join(out))
}

fn links_in(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for ev in Parser::new_ext(body, Options::all()) {
        match ev {
            Event::Start(Tag::Link { dest_url, .. })
            | Event::Start(Tag::Image { dest_url, .. }) => {
                out.push(dest_url.to_string());
            }
            _ => {}
        }
    }
    out
}

pub fn run(root: &Path, opts: &CheckOptions) -> Vec<Finding> {
    run_with(root, opts, &index::scan_full(root))
}

/// Checks over an existing scan, so callers that already walked the tree (submit) do not
/// read every document again (review P5).
pub fn run_with(root: &Path, opts: &CheckOptions, scan: &index::Scan) -> Vec<Finding> {
    let mut findings = Vec::new();

    for (doc, body) in scan.docs.iter().zip(scan.bodies.iter()) {
        if let Some(err) = &doc.frontmatter_error {
            findings.push(Finding {
                level: Level::Error,
                kind: Kind::InvalidFrontmatter,
                path: doc.path.clone(),
                message: err.clone(),
            });
        }
        for link in links_in(body) {
            if let Some(p) = resolve_link(root, &doc.path, &link) {
                if !p.exists() {
                    findings.push(Finding {
                        level: Level::Error,
                        kind: Kind::BrokenLink,
                        path: doc.path.clone(),
                        message: format!("link target not found: {link}"),
                    });
                }
            }
        }
    }

    for (rel, size) in &scan.files {
        let in_assets = rel.split('/').any(|seg| seg == "assets");
        if !in_assets {
            continue;
        }
        let ext = Path::new(rel)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if !IMAGE_EXT.contains(&ext.as_str()) {
            findings.push(Finding {
                level: Level::Warning,
                kind: Kind::NonImageAsset,
                path: rel.clone(),
                message: format!("not an image: .{ext}"),
            });
        }
        if *size > opts.asset_cap {
            findings.push(Finding {
                level: Level::Error,
                kind: Kind::OversizeAsset,
                path: rel.clone(),
                message: format!("{size} bytes exceeds cap of {} bytes", opts.asset_cap),
            });
        }
    }

    // Repo-controlled agent configuration executes on the machine of whoever opens a thread.
    // kmdn launches Claude with user settings only; pi and Codex may still read these (review S7).
    for dir in [".claude", ".pi", ".codex", ".agents", ".cursor", ".gemini"] {
        if root.join(dir).is_dir() {
            findings.push(Finding {
                level: Level::Warning,
                kind: Kind::AgentConfig,
                path: format!("{dir}/"),
                message: format!("{dir}/ holds agent configuration or hooks that run for everyone who opens a thread; keep it out of a shared knowledge base"),
            });
        }
    }

    if index::agents_md_is_stale_for(root, &scan.docs) {
        findings.push(Finding {
            level: Level::Error,
            kind: Kind::StaleIndex,
            path: index::AGENTS_FILE.into(),
            message: "AGENTS.md does not match the documents; run `kmdn-cli index`".into(),
        });
    }

    findings.sort_by(|a, b| a.path.cmp(&b.path).then(a.message.cmp(&b.message)));
    findings
}

pub fn has_errors(f: &[Finding]) -> bool {
    f.iter().any(|x| x.level == Level::Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_each_kind() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        std::fs::create_dir_all(r.join("ops/assets/deploy")).unwrap();
        std::fs::write(r.join("ops/deploy.md"), "---\ntitle: Deploy\n---\nSee [rollback](rollback.md) and [gone](missing.md#x) and ![img](assets/deploy/a.png) and [ext](https://x.y/z).\n").unwrap();
        std::fs::write(r.join("ops/rollback.md"), "---\ntitle: [\n---\n").unwrap();
        std::fs::write(r.join("ops/assets/deploy/a.png"), vec![0u8; 10]).unwrap();
        std::fs::write(r.join("ops/assets/deploy/big.pdf"), vec![0u8; 20]).unwrap();
        std::fs::create_dir_all(r.join(".claude")).unwrap();
        let f = run(r, &CheckOptions { asset_cap: 15 });
        let kinds: Vec<Kind> = f.iter().map(|x| x.kind).collect();
        assert!(kinds.contains(&Kind::AgentConfig));
        assert!(kinds.contains(&Kind::BrokenLink), "{f:?}");
        assert!(kinds.contains(&Kind::InvalidFrontmatter));
        assert!(kinds.contains(&Kind::OversizeAsset));
        assert!(kinds.contains(&Kind::NonImageAsset));
        assert!(kinds.contains(&Kind::StaleIndex));
        assert_eq!(f.iter().filter(|x| x.kind == Kind::BrokenLink).count(), 1);
        assert!(has_errors(&f));

        index::write_agents_md(r).unwrap();
        std::fs::write(r.join("ops/missing.md"), "# m\n").unwrap();
        std::fs::write(r.join("ops/rollback.md"), "# ok\n").unwrap();
        index::write_agents_md(r).unwrap();
        let f = run(r, &CheckOptions::default_cap());
        assert_eq!(
            f.iter().filter(|x| x.level == Level::Error).count(),
            0,
            "{f:?}"
        );
    }

    #[test]
    fn links_escaping_root_are_ignored_not_flagged() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("a.md"),
            "[up](../../etc/passwd) [abs](/root.md)\n",
        )
        .unwrap();
        index::write_agents_md(d.path()).unwrap();
        let f = run(d.path(), &CheckOptions::default_cap());
        assert!(f.is_empty(), "{f:?}");
    }
}
