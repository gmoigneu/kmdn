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
use kmdn_core::commit::allowed_set;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DetectedAgent {
    pub kind: AgentKind,
    pub available: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct AgentEnvelope {
    pub slug: String,
    pub kind: AgentKind,
    pub event: AgentEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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
    prompt_counter: Mutex<u64>,
    policy: Arc<Policy>,
    /// Tool call id -> paths, for writes that have started but not finished.
    writes_in_flight: Mutex<HashMap<String, Vec<String>>>,
    pending_file: PathBuf,
}

/// Agent edits waiting for the user's accept or revert (D15). Persisted per thread so a
/// restart does not lose them, and readable without a running session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingEdits {
    pub agent: Option<AgentKind>,
    pub request: String,
    pub paths: Vec<String>,
}

pub fn session_dir(data_dir: &Path, slug: &str) -> PathBuf {
    data_dir.join("agent-sessions").join(slug)
}

pub fn read_pending(data_dir: &Path, slug: &str) -> PendingEdits {
    std::fs::read_to_string(session_dir(data_dir, slug).join("pending.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_pending(data_dir: &Path, slug: &str, p: &PendingEdits) {
    let dir = session_dir(data_dir, slug);
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_vec_pretty(p) {
        let _ = std::fs::write(dir.join("pending.json"), json);
    }
}

/// Drops `paths` from the pending list after an accept or revert.
pub fn clear_pending(data_dir: &Path, slug: &str, paths: &[String]) -> PendingEdits {
    let mut p = read_pending(data_dir, slug);
    p.paths.retain(|x| !paths.contains(x));
    write_pending(data_dir, slug, &p);
    p
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
    #[allow(clippy::too_many_arguments)]
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
        let session_dir = session_dir(data_dir, slug);
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

        let policy = Arc::new(Policy {
            mode,
            allowed: allowed_set(agents::AGENT_ALLOWED).map_err(|e| e.to_string())?,
            worktree: worktree.to_path_buf(),
        });
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
            policy: policy.clone(),
            writes_in_flight: Mutex::new(HashMap::new()),
            pending_file: session_dir.join("pending.json"),
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
                    s2.track_write(&ev);
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
                        s2.persist_log();
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
        s.remember_request(text);
        let line = match s.kind {
            AgentKind::Claude => agents::claude::user_message(text),
            AgentKind::Codex => {
                // thread/start is answered on the stdout pump, so the first prompt can arrive
                // before the thread id does. Wait for it instead of failing the message.
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
                loop {
                    let line = s.codex.lock().unwrap().turn_start(text);
                    match line {
                        Some(l) => break l,
                        None if std::time::Instant::now() < deadline => {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await
                        }
                        None => return Err("codex did not start its thread within 20s".into()),
                    }
                }
            }
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
        // Logged only once the message can actually go out, so a prompt that never reached the
        // agent does not end up in the condensed log posted to the pull request.
        s.log.lock().unwrap().user(s.kind, text);
        s.persist_log();
        s.append_transcript(&serde_json::json!({ "user": text }));
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

    /// Live session log, or the persisted one from an earlier session.
    pub fn condensed_log(&self, data_dir: &Path, slug: &str) -> Option<String> {
        let live = self
            .sessions
            .lock()
            .unwrap()
            .get(slug)
            .map(|s| s.log.lock().unwrap().render())
            .filter(|l| !l.trim().is_empty());
        live.or_else(|| {
            std::fs::read_to_string(session_dir(data_dir, slug).join("log.md"))
                .ok()
                .filter(|l| !l.trim().is_empty())
        })
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
        // A held lock means a send or stop is in flight, so the process is still ours.
        let running = match self.child.try_lock() {
            Ok(mut child) => matches!(child.try_wait(), Ok(None)),
            Err(_) => true,
        };
        SessionInfo {
            slug: self.slug.clone(),
            kind: self.kind,
            mode: self.mode,
            session_id: self.session_id.lock().unwrap().clone(),
            running,
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

    fn pending(&self) -> PendingEdits {
        std::fs::read_to_string(&self.pending_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_pending(&self, p: &PendingEdits) {
        if let Ok(json) = serde_json::to_vec_pretty(p) {
            let _ = std::fs::write(&self.pending_file, json);
        }
    }

    /// The condensed log survives restarts so the submit preview can show it later (D58).
    fn persist_log(&self) {
        let rendered = self.log.lock().unwrap().render();
        let _ = std::fs::write(self.pending_file.with_file_name("log.md"), rendered);
    }

    fn remember_request(&self, text: &str) {
        let mut p = self.pending();
        p.agent = Some(self.kind);
        p.request = text
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(120)
            .collect();
        self.save_pending(&p);
    }

    /// Writes that completed successfully become pending edits (D15).
    fn track_write(&self, ev: &AgentEvent) {
        match ev {
            AgentEvent::ToolCallStarted {
                id,
                kind: ToolKind::Write,
                paths,
                ..
            } => {
                self.writes_in_flight
                    .lock()
                    .unwrap()
                    .insert(id.clone(), paths.clone());
            }
            AgentEvent::ToolCallFinished { id, ok, .. } => {
                let paths = self.writes_in_flight.lock().unwrap().remove(id);
                if let (Some(paths), true) = (paths, *ok) {
                    let mut p = self.pending();
                    p.agent = Some(self.kind);
                    for raw in paths {
                        if let Some(rel) = self.policy.relative(&raw) {
                            if !p.paths.contains(&rel) {
                                p.paths.push(rel);
                            }
                        }
                    }
                    self.save_pending(&p);
                }
            }
            _ => {}
        }
    }

    fn record(&self, ev: &AgentEvent) {
        self.turn_events.lock().unwrap().push(ev.clone());
        if !matches!(ev, AgentEvent::TextDelta { .. }) {
            self.append_transcript(&serde_json::to_value(ev).unwrap_or_default());
        }
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

/// One-shot, read-only request to an agent CLI for a PR title and summary (D26). Runs the
/// user's own login, never writes, and has a hard timeout. The diff is truncated so the
/// prompt stays small.
pub async fn suggest_draft(
    kind: AgentKind,
    worktree: &Path,
    diff: &str,
) -> Result<(String, String), String> {
    let bin = which(kind.binary()).ok_or_else(|| format!("{} is not installed", kind.binary()))?;
    let excerpt: String = diff.chars().take(12_000).collect();
    let prompt = format!(
        "You are helping open a pull request on a markdown knowledge base. Below is what changed.\n\
Reply with exactly two lines and nothing else:\nTITLE: <imperative, under 70 characters, no trailing period>\n\
SUMMARY: <one to three plain sentences for a reviewer>\n\n{excerpt}"
    );
    let mut cmd = Command::new(&bin);
    cmd.current_dir(worktree)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    cmd.env_remove("CLAUDECODE");
    let tmp = std::env::temp_dir().join(format!("kmdn-suggest-{}.txt", uuid::Uuid::new_v4()));
    match kind {
        AgentKind::Claude => {
            cmd.args([
                "-p",
                "--output-format",
                "json",
                "--permission-mode",
                "plan",
                "--setting-sources",
                "user",
                "--disallowedTools",
                "Bash",
                "Edit",
                "Write",
                "MultiEdit",
                "NotebookEdit",
            ]);
        }
        AgentKind::Codex => {
            cmd.args([
                "exec",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "-o",
            ])
            .arg(&tmp)
            .arg(&prompt);
        }
        AgentKind::Pi => {
            cmd.args(["-p", "--no-session", "--no-tools", &prompt]);
        }
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if kind == AgentKind::Claude {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
        }
    } else {
        drop(child.stdin.take());
    }
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| "the agent took more than two minutes".to_string())?
    .map_err(|e| e.to_string())?;
    let text = match kind {
        AgentKind::Claude => {
            let v: serde_json::Value =
                serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
            v.get("result")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string()
        }
        AgentKind::Codex => std::fs::read_to_string(&tmp).unwrap_or_default(),
        AgentKind::Pi => String::from_utf8_lossy(&out.stdout).to_string(),
    };
    let _ = std::fs::remove_file(&tmp);
    parse_suggestion(&text)
        .ok_or_else(|| format!("{} did not answer in the expected format", kind.label()))
}

/// Pulls TITLE and SUMMARY out of the reply; tolerates extra prose around them.
pub fn parse_suggestion(text: &str) -> Option<(String, String)> {
    let mut title = None;
    let mut summary: Vec<String> = Vec::new();
    let mut in_summary = false;
    for line in text.lines() {
        let l = line.trim().trim_start_matches(['*', '-', '#']).trim();
        if let Some(t) = l.strip_prefix("TITLE:") {
            title = Some(
                t.trim()
                    .trim_matches(['"', '*'])
                    .trim_end_matches('.')
                    .chars()
                    .take(70)
                    .collect::<String>(),
            );
            in_summary = false;
        } else if let Some(sm) = l.strip_prefix("SUMMARY:") {
            summary.push(sm.trim().to_string());
            in_summary = true;
        } else if in_summary && !l.is_empty() {
            summary.push(l.to_string());
        }
    }
    let title = title.filter(|t| !t.is_empty())?;
    Some((title, summary.join(" ").trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::parse_suggestion;

    #[test]
    fn parses_title_and_summary_with_noise() {
        let (t, s) = parse_suggestion("Sure.\nTITLE: Update the deploy runbook.\nSUMMARY: Adds a rollback step.\nAlso notes the timeout.\n").unwrap();
        assert_eq!(t, "Update the deploy runbook");
        assert_eq!(s, "Adds a rollback step. Also notes the timeout.");
        assert!(parse_suggestion("no markers here").is_none());
    }
}
