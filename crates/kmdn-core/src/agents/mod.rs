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

impl Policy {
    fn relative(&self, p: &str) -> String {
        let path = std::path::Path::new(p);
        match path.strip_prefix(&self.worktree) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => p.trim_start_matches("./").to_string(),
        }
    }

    pub fn path_allowed(&self, p: &str) -> bool {
        let rel = self.relative(p);
        let escapes = std::path::Path::new(p).is_absolute()
            && !std::path::Path::new(p).starts_with(&self.worktree);
        !escapes
            && !rel.starts_with("..")
            && self.allowed.is_match(&rel)
            && rel != crate::index::AGENTS_FILE
            && !rel.starts_with(".kmdn/")
    }

    /// Decides a tool call without asking the user. `None` means the UI must ask.
    pub fn decide(&self, kind: &ToolKind, paths: &[String]) -> Option<PermissionReply> {
        match kind {
            ToolKind::Read => Some(PermissionReply::Allow),
            ToolKind::Write => {
                if self.mode == Mode::Suggest {
                    return Some(PermissionReply::Deny { reason: "Suggest mode: propose the change in your reply instead of writing files".into() });
                }
                let bad: Vec<&String> = paths.iter().filter(|p| !self.path_allowed(p)).collect();
                if bad.is_empty() {
                    Some(PermissionReply::Allow)
                } else {
                    Some(PermissionReply::Deny {
                        reason: format!(
                            "kmdn only allows writes to markdown documents and assets, not: {}",
                            bad.iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
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
    use crate::commit::{allowed_set, DEFAULT_ALLOWED};

    fn policy(mode: Mode) -> Policy {
        Policy {
            mode,
            allowed: allowed_set(DEFAULT_ALLOWED).unwrap(),
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
        assert!(matches!(
            p.decide(&ToolKind::Write, &["/kb/scratch.txt".into()]),
            Some(PermissionReply::Deny { .. })
        ));
        assert!(matches!(
            p.decide(&ToolKind::Write, &["/kb/AGENTS.md".into()]),
            Some(PermissionReply::Deny { .. })
        ));
        assert!(matches!(
            p.decide(&ToolKind::Write, &["/etc/passwd.md".into()]),
            Some(PermissionReply::Deny { .. })
        ));
        assert!(matches!(
            p.decide(&ToolKind::Write, &["../other/x.md".into()]),
            Some(PermissionReply::Deny { .. })
        ));
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
    fn suggest_never_writes_and_developer_asks_for_shell() {
        assert!(matches!(
            policy(Mode::Suggest).decide(&ToolKind::Write, &["/kb/a.md".into()]),
            Some(PermissionReply::Deny { .. })
        ));
        assert_eq!(policy(Mode::Developer).decide(&ToolKind::Shell, &[]), None);
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
