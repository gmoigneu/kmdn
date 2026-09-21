//! Agent adapters (D55, 06-agents.md). One internal event model; one adapter per agent that
//! translates its wire format. Parsers here are pure so they can be replayed from fixtures.
//! Process management lives in the desktop app, which owns the async runtime.

pub mod claude;
pub mod codex;
pub mod pi;

use globset::GlobSet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    Codex,
    Pi,
}

impl AgentKind {
    pub fn binary(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Pi => "pi",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude",
            AgentKind::Codex => "Codex",
            AgentKind::Pi => "pi",
        }
    }
}

/// Composer mode (D46). Suggest never writes. Edit writes markdown and assets. Developer adds shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Suggest,
    Edit,
    Developer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Write,
    Shell,
    Other(String),
}

/// Normalized event stream every adapter produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted {
        session_id: String,
    },
    TextDelta {
        text: String,
    },
    /// A whole assistant message when the agent does not stream deltas.
    Text {
        text: String,
    },
    ToolCallStarted {
        id: String,
        kind: ToolKind,
        paths: Vec<String>,
        command: Option<String>,
    },
    ToolCallFinished {
        id: String,
        ok: bool,
        summary: String,
    },
    /// The host must answer with `PermissionReply` through the adapter.
    PermissionRequest {
        id: String,
        kind: ToolKind,
        paths: Vec<String>,
        command: Option<String>,
    },
    TurnDone,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PermissionReply {
    Allow,
    Deny { reason: String },
}

/// The kmdn policy table (06-agents.md, Permission gate).
pub struct Policy {
    pub mode: Mode,
    pub allowed: GlobSet,
    pub worktree: std::path::PathBuf,
}

/// Globs an agent may write. Narrower than the commit filter: `.kmdn/` is configuration.
pub const AGENT_ALLOWED: &[&str] = &["**/*.md", "*.md", "**/assets/**", "assets/**"];

/// Lexically normalizes a path: resolves `.` and `..`, keeps it absolute. None if `..`
/// climbs above the root.
fn lexical_normalize(p: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => out.push(c.as_os_str()),
            Component::Normal(n) => out.push(n),
        }
    }
    Some(out)
}

impl Policy {
    /// Worktree-relative, normalized, forward-slash path. None when the path escapes the
    /// worktree (absolute elsewhere, `..` climbing out) or passes through a symlink.
    pub fn relative(&self, p: &str) -> Option<String> {
        let path = std::path::Path::new(p);
        if p.is_empty() || p.starts_with('~') {
            return None;
        }
        let wt = lexical_normalize(&self.worktree)?;
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            wt.join(path)
        };
        let abs = lexical_normalize(&abs)?;
        let rel = abs.strip_prefix(&wt).ok()?;
        if rel.as_os_str().is_empty() {
            return None;
        }
        // A symlink anywhere between the worktree and the target would redirect the write.
        let mut cursor = wt.clone();
        for c in rel.components() {
            cursor.push(c);
            if let Ok(meta) = std::fs::symlink_metadata(&cursor) {
                if meta.file_type().is_symlink() {
                    return None;
                }
            } else {
                break; // does not exist yet; nothing below it can be a symlink either
            }
        }
        Some(rel.to_string_lossy().replace('\\', "/"))
    }

    fn is_protected(rel: &str) -> bool {
        let first = rel.split('/').next().unwrap_or("");
        rel.eq_ignore_ascii_case(crate::index::AGENTS_FILE)
            || first.eq_ignore_ascii_case(crate::index::CONFIG_DIR)
            || first.eq_ignore_ascii_case(".git")
    }

    pub fn path_allowed(&self, p: &str) -> bool {
        match self.relative(p) {
            Some(rel) => !Self::is_protected(&rel) && self.allowed.is_match(&rel),
            None => false,
        }
    }

    /// Decides a tool call without asking the user. `None` means the UI must ask.
    pub fn decide(&self, kind: &ToolKind, paths: &[String]) -> Option<PermissionReply> {
        match kind {
            ToolKind::Read => Some(PermissionReply::Allow),
            ToolKind::Write => {
                if self.mode == Mode::Suggest {
                    return Some(PermissionReply::Deny {
                        reason: "Suggest mode: propose the change in your reply instead of writing files".into(),
                    });
                }
                if paths.is_empty() {
                    // Unknown target: never auto-approve a write we cannot see (review S4).
                    return None;
                }
                let bad: Vec<&String> = paths.iter().filter(|p| !self.path_allowed(p)).collect();
                if bad.is_empty() {
                    Some(PermissionReply::Allow)
                } else {
                    Some(PermissionReply::Deny {
                        reason: format!(
                            "kmdn only allows writes to markdown documents and assets inside the knowledge base, not: {}",
                            bad.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                    })
                }
            }
            ToolKind::Shell => match self.mode {
                Mode::Developer => None,
                _ => Some(PermissionReply::Deny {
                    reason: "shell is disabled outside Developer mode".into(),
                }),
            },
            ToolKind::Other(_) => None,
        }
    }
}

/// Condensed log posted to the PR (D58): user prompts, one line per agent turn, files touched.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CondensedLog {
    pub entries: Vec<String>,
}

impl CondensedLog {
    pub fn user(&mut self, agent: AgentKind, text: &str) {
        let first = text
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(200)
            .collect::<String>();
        self.entries
            .push(format!("- **You** to {}: {first}", agent.label()));
    }
    pub fn from_events(&mut self, agent: AgentKind, events: &[AgentEvent]) {
        let mut files: Vec<String> = Vec::new();
        let mut denied = 0;
        for e in events {
            match e {
                AgentEvent::ToolCallStarted {
                    kind: ToolKind::Write,
                    paths,
                    ..
                } => files.extend(paths.iter().cloned()),
                AgentEvent::ToolCallFinished { ok: false, .. } => denied += 1,
                _ => {}
            }
        }
        files.sort();
        files.dedup();
        let mut line = format!("- **{}**", agent.label());
        if !files.is_empty() {
            line.push_str(&format!(
                " edited {}",
                files
                    .iter()
                    .map(|f| format!("`{f}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        } else {
            line.push_str(" replied without editing files");
        }
        if denied > 0 {
            line.push_str(&format!(
                " ({denied} call{} blocked)",
                if denied == 1 { "" } else { "s" }
            ));
        }
        self.entries.push(line);
    }
    pub fn render(&self) -> String {
        self.entries.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::allowed_set;

    fn policy(mode: Mode) -> Policy {
        Policy {
            mode,
            allowed: allowed_set(AGENT_ALLOWED).unwrap(),
            worktree: "/kb".into(),
        }
    }

    #[test]
    fn policy_allows_markdown_and_assets_only() {
        let p = policy(Mode::Edit);
        assert_eq!(
            p.decide(&ToolKind::Write, &["/kb/notes.md".into()]),
            Some(PermissionReply::Allow)
        );
        assert_eq!(
            p.decide(&ToolKind::Write, &["docs/assets/x/a.png".into()]),
            Some(PermissionReply::Allow)
        );
        assert_eq!(
            p.decide(&ToolKind::Write, &["./docs/./guide.md".into()]),
            Some(PermissionReply::Allow)
        );
        for bad in [
            "/kb/scratch.txt",
            "/kb/AGENTS.md",
            "/kb/agents.md",
            "/kb/docs/../AGENTS.md",
            "/kb/x/../.kmdn/config.yaml",
            "/kb/.KMDN/config.md",
            "/kb/.git/hooks/pre-commit.md",
            "/etc/passwd.md",
            "../other/x.md",
            "docs/../../other/x.md",
            "/kb/assets/../../../home/u/.bashrc",
            "~/notes.md",
            "",
        ] {
            assert!(
                matches!(
                    p.decide(&ToolKind::Write, &[bad.into()]),
                    Some(PermissionReply::Deny { .. })
                ),
                "{bad} should be denied"
            );
        }
        assert!(matches!(
            p.decide(&ToolKind::Shell, &[]),
            Some(PermissionReply::Deny { .. })
        ));
        assert_eq!(
            p.decide(&ToolKind::Read, &["/kb/.kmdn/config.yaml".into()]),
            Some(PermissionReply::Allow)
        );
    }

    #[test]
    fn writes_with_unknown_paths_are_not_auto_approved() {
        assert_eq!(policy(Mode::Edit).decide(&ToolKind::Write, &[]), None);
    }

    #[test]
    fn symlinked_directories_inside_the_worktree_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let wt = d.path().join("kb");
        std::fs::create_dir_all(wt.join("docs")).unwrap();
        let outside = d.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, wt.join("linked")).unwrap();
        let p = Policy {
            mode: Mode::Edit,
            allowed: allowed_set(AGENT_ALLOWED).unwrap(),
            worktree: wt.clone(),
        };
        assert!(p.path_allowed("docs/new.md"));
        #[cfg(unix)]
        assert!(
            !p.path_allowed("linked/new.md"),
            "symlink escape must be denied"
        );
        assert_eq!(
            p.relative(&wt.join("docs/a.md").to_string_lossy())
                .as_deref(),
            Some("docs/a.md")
        );
    }

    #[test]
    fn condensed_log_reads_well() {
        let mut log = CondensedLog::default();
        log.user(AgentKind::Codex, "Create two files\nmore detail");
        log.from_events(
            AgentKind::Codex,
            &[
                AgentEvent::ToolCallStarted {
                    id: "1".into(),
                    kind: ToolKind::Write,
                    paths: vec!["notes.md".into()],
                    command: None,
                },
                AgentEvent::ToolCallFinished {
                    id: "1".into(),
                    ok: true,
                    summary: String::new(),
                },
                AgentEvent::ToolCallStarted {
                    id: "2".into(),
                    kind: ToolKind::Write,
                    paths: vec!["scratch.txt".into()],
                    command: None,
                },
                AgentEvent::ToolCallFinished {
                    id: "2".into(),
                    ok: false,
                    summary: "denied".into(),
                },
            ],
        );
        assert_eq!(log.render(), "- **You** to Codex: Create two files\n- **Codex** edited `notes.md`, `scratch.txt` (1 call blocked)");
    }
}
