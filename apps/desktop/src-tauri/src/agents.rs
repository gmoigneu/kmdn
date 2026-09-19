//! Agent runtime (06-agents.md): one child process per thread session, stdio pumped on the
//! Tauri async runtime, normalized events forwarded to the window as `agent://event`.
//! Permission requests are decided by the kmdn policy first; only undecided ones reach the UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use kmdn_core::agents::{
    self, codex::CodexState, AgentEvent, AgentKind, CondensedLog, Mode, PermissionReply, Policy,
    ToolKind,
};
use kmdn_core::commit::{allowed_set, DEFAULT_ALLOWED};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone, Serialize)]
pub struct DetectedAgent {
    pub kind: AgentKind,
    pub available: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentEnvelope {
    pub slug: String,
    pub kind: AgentKind,
    pub event: AgentEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub slug: String,
    pub kind: AgentKind,
    pub mode: Mode,
    pub session_id: Option<String>,
    pub running: bool,
}

pub struct Session {
    kind: AgentKind,
    mode: Mode,
    slug: String,
    child: AsyncMutex<Child>,
    stdin: AsyncMutex<ChildStdin>,
    codex: Mutex<CodexState>,
    session_id: Mutex<Option<String>>,
    pub log: Mutex<CondensedLog>,
    turn_events: Mutex<Vec<AgentEvent>>,
    transcript: PathBuf,
    /// Codex: pending request id -> item id is inside CodexState. Others: nothing to keep.
    prompt_counter: Mutex<u64>,
}

#[derive(Default)]
pub struct Runtime {
    pub sessions: Mutex<HashMap<String, Arc<Session>>>,
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

pub async fn detect() -> Vec<DetectedAgent> {
    let mut out = Vec::new();
    for kind in [AgentKind::Claude, AgentKind::Codex, AgentKind::Pi] {
        let path = which(kind.binary());
        let version = match &path {
            Some(p) => Command::new(p)
                .arg("--version")
                .output()
                .await
                .ok()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string()
                })
                .filter(|v| !v.is_empty()),
            None => None,
        };
        out.push(DetectedAgent {
            kind,
            available: path.is_some(),
            version,
            path: path.map(|p| p.to_string_lossy().to_string()),
        });
    }
    out
}

fn system_context(worktree: &Path, mode: Mode) -> String {
    let rules = match mode {
        Mode::Suggest => "You are in Suggest mode: do not write files. Propose the exact markdown to change in your reply.",
        Mode::Edit => "You may create and edit markdown documents (*.md) and images under assets/ folders. Nothing else is writable.",
        Mode::Developer => "You may edit markdown documents and assets, and run shell commands when approved.",
    };
    format!(
        "This is a kmdn knowledge base: a git repository of GitHub Flavored Markdown documents with optional YAML frontmatter (title, description, owner, tags, status, order, reviewed). Links are relative paths. Images live in assets/ next to the document. Read AGENTS.md first for the map of documents. Never edit AGENTS.md or anything under .kmdn/; they are generated or configuration. Working directory: {}. {rules}",
        worktree.display()
    )
}

fn emit(app: &AppHandle, env: &AgentEnvelope) {
    let _ = app.emit("agent://event", env);
}

impl Runtime {
    pub async fn start(
        &self,
        app: AppHandle,
        data_dir: &Path,
        slug: &str,
        worktree: &Path,
        kind: AgentKind,
        mode: Mode,
        resume: Option<String>,
    ) -> Result<SessionInfo, String> {
        self.stop(slug).await;
        let bin = which(kind.binary())
            .ok_or_else(|| format!("{} is not installed or not on PATH", kind.binary()))?;
        let session_dir = data_dir.join("agent-sessions").join(slug);
        std::fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&bin);
        cmd.current_dir(worktree)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd.env_remove("CLAUDECODE");
        match kind {
            AgentKind::Claude => {
                cmd.args(agents::claude::args(mode, worktree, resume.as_deref()));
            }
            AgentKind::Codex => {
                cmd.arg("app-server");
            }
            AgentKind::Pi => {
                let gate = agents::pi::write_gate(&session_dir, mode).map_err(|e| e.to_string())?;
                let id = resume
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                cmd.args(agents::pi::args(&gate, &session_dir, Some(&id)));
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to start {}: {e}", kind.binary()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take();

        let session = Arc::new(Session {
            kind,
            mode,
            slug: slug.to_string(),
            child: AsyncMutex::new(child),
            stdin: AsyncMutex::new(stdin),
            codex: Mutex::new(CodexState::default()),
            session_id: Mutex::new(resume.clone()),
            log: Mutex::new(CondensedLog::default()),
            turn_events: Mutex::new(Vec::new()),
            transcript: session_dir.join("transcript.jsonl"),
            prompt_counter: Mutex::new(0),
        });
        self.sessions
            .lock()
            .unwrap()
            .insert(slug.to_string(), session.clone());

        // Handshake before the first prompt.
        match kind {
            AgentKind::Codex => {
                let (init, initialized, start) = {
                    let mut st = session.codex.lock().unwrap();
                    (
                        st.initialize(),
                        st.initialized(),
                        st.thread_start(worktree, mode, resume.as_deref()),
                    )
                };
                session.write_line(&init).await?;
                session.write_line(&initialized).await?;
                session.write_line(&start).await?;
            }
            AgentKind::Claude => {
                // First user message carries the KB context; later prompts are plain.
                session
                    .write_line(&agents::claude::user_message(&system_context(
                        worktree, mode,
                    )))
                    .await?;
            }
            AgentKind::Pi => {}
        }

        // stdout pump
        let policy = Policy {
            mode,
            allowed: allowed_set(DEFAULT_ALLOWED).map_err(|e| e.to_string())?,
            worktree: worktree.to_path_buf(),
        };
        let s2 = session.clone();
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let events = match s2.kind {
                    AgentKind::Claude => agents::claude::parse_line(&line),
                    AgentKind::Codex => s2.codex.lock().unwrap().parse_line(&line),
                    AgentKind::Pi => agents::pi::parse_line(&line),
                };
                for ev in events {
                    s2.record(&ev);
                    if let AgentEvent::SessionStarted { session_id } = &ev {
                        *s2.session_id.lock().unwrap() = Some(session_id.clone());
                    }
                    if let AgentEvent::PermissionRequest {
                        id, kind, paths, ..
                    } = &ev
                    {
                        if let Some(decision) = policy.decide(kind, paths) {
                            let allow = matches!(decision, PermissionReply::Allow);
                            let reason = match &decision {
                                PermissionReply::Deny { reason } => reason.clone(),
                                _ => String::new(),
                            };
                            let _ = s2.reply(id, allow, &reason).await;
                            // Tell the UI what happened without asking it anything.
                            emit(
                                &app2,
                                &AgentEnvelope {
                                    slug: s2.slug.clone(),
                                    kind: s2.kind,
                                    event: AgentEvent::ToolCallFinished {
                                        id: id.clone(),
                                        ok: allow,
                                        summary: if allow {
                                            "auto-approved".into()
                                        } else {
                                            reason
                                        },
                                    },
                                },
                            );
                            continue;
                        }
                    }
                    if matches!(ev, AgentEvent::TurnDone) {
                        let turn = std::mem::take(&mut *s2.turn_events.lock().unwrap());
                        s2.log.lock().unwrap().from_events(s2.kind, &turn);
                    }
                    emit(
                        &app2,
                        &AgentEnvelope {
                            slug: s2.slug.clone(),
                            kind: s2.kind,
                            event: ev,
                        },
                    );
                }
            }
            emit(
                &app2,
                &AgentEnvelope {
                    slug: s2.slug.clone(),
                    kind: s2.kind,
                    event: AgentEvent::Error {
                        message: "agent process ended".into(),
                    },
                },
            );
        });
        if let Some(stderr) = stderr {
            let s3 = session.clone();
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    s3.append_transcript(&serde_json::json!({ "stderr": line }));
                }
            });
        }
        // pi confirms its session id on request.
        if kind == AgentKind::Pi {
            session
                .write_line(&serde_json::json!({ "id": "state", "type": "get_state" }).to_string())
                .await?;
        }
        Ok(session.info())
    }

    pub async fn send(&self, slug: &str, text: &str) -> Result<(), String> {
        let s = self.get(slug)?;
        s.log.lock().unwrap().user(s.kind, text);
        s.append_transcript(&serde_json::json!({ "user": text }));
        let line = match s.kind {
            AgentKind::Claude => agents::claude::user_message(text),
            AgentKind::Codex => s
                .codex
                .lock()
                .unwrap()
                .turn_start(text)
                .ok_or("codex thread not ready yet")?,
            AgentKind::Pi => {
                let n = {
                    let mut c = s.prompt_counter.lock().unwrap();
                    *c += 1;
                    *c
                };
                let ctx = if n == 1 {
                    format!("{}\n\n{text}", system_context(Path::new("."), s.mode))
                } else {
                    text.to_string()
                };
                agents::pi::prompt(&ctx, &format!("p{n}"))
            }
        };
        s.write_line(&line).await
    }

    pub async fn reply_permission(
        &self,
        slug: &str,
        id: &str,
        allow: bool,
        reason: &str,
    ) -> Result<(), String> {
        self.get(slug)?.reply(id, allow, reason).await
    }

    pub async fn cancel(&self, slug: &str) -> Result<(), String> {
        let s = self.get(slug)?;
        match s.kind {
            AgentKind::Codex => {
                let line = s.codex.lock().unwrap().turn_interrupt();
                if let Some(l) = line { s.write_line(&l).await?; }
                Ok(())
            }
            AgentKind::Pi => s.write_line(&agents::pi::abort()).await,
            AgentKind::Claude => s.write_line(&serde_json::json!({ "type": "control_request", "request_id": uuid::Uuid::new_v4().to_string(), "request": { "subtype": "interrupt" } }).to_string()).await,
        }
    }

    pub async fn stop(&self, slug: &str) {
        let s = self.sessions.lock().unwrap().remove(slug);
        if let Some(s) = s {
            let _ = s.child.lock().await.kill().await;
        }
    }

    pub fn info(&self, slug: &str) -> Option<SessionInfo> {
        self.sessions.lock().unwrap().get(slug).map(|s| s.info())
    }

    pub fn condensed_log(&self, slug: &str) -> Option<String> {
        self.sessions
            .lock()
            .unwrap()
            .get(slug)
            .map(|s| s.log.lock().unwrap().render())
    }

    fn get(&self, slug: &str) -> Result<Arc<Session>, String> {
        self.sessions
            .lock()
            .unwrap()
            .get(slug)
            .cloned()
            .ok_or_else(|| format!("no agent session for thread {slug}"))
    }
}

impl Session {
    fn info(&self) -> SessionInfo {
        SessionInfo {
            slug: self.slug.clone(),
            kind: self.kind,
            mode: self.mode,
            session_id: self.session_id.lock().unwrap().clone(),
            running: true,
        }
    }

    async fn write_line(&self, line: &str) -> Result<(), String> {
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())
    }

    async fn reply(&self, id: &str, allow: bool, reason: &str) -> Result<(), String> {
        let line = match self.kind {
            AgentKind::Claude => agents::claude::permission_response(id, allow, reason, None),
            AgentKind::Codex => self.codex.lock().unwrap().approval_response(id, allow),
            AgentKind::Pi => agents::pi::ui_response(id, allow),
        };
        self.write_line(&line).await
    }

    fn record(&self, ev: &AgentEvent) {
        self.turn_events.lock().unwrap().push(ev.clone());
        if !matches!(ev, AgentEvent::TextDelta { .. }) {
            self.append_transcript(&serde_json::to_value(ev).unwrap_or_default());
        }
        let _ = ToolKind::Read; // keep the import meaningful for readers of this file
    }

    fn append_transcript(&self, v: &serde_json::Value) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.transcript)
        {
            use std::io::Write;
            let _ = writeln!(f, "{v}");
        }
    }
}
