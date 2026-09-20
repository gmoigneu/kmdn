//! Codex adapter: `codex app-server`, JSON-RPC 2.0 over stdio. Verified in spikes/agents/README.md.
//! Approval requests carry only an `itemId`; paths come from the matching `item/started`.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use super::{AgentEvent, Mode, ToolKind};

#[derive(Default)]
pub struct CodexState {
    next_id: u64,
    /// itemId -> paths (fileChange) or command (commandExecution)
    items: HashMap<String, (ToolKind, Vec<String>, Option<String>)>,
    /// JSON-RPC id of a pending server request -> itemId
    pending: HashMap<String, String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
}

impl CodexState {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// First message on stdin.
    pub fn initialize(&mut self) -> String {
        let id = self.id();
        json!({ "jsonrpc": "2.0", "id": id, "method": "initialize", "params": { "clientInfo": { "name": "kmdn", "title": "kmdn", "version": env!("CARGO_PKG_VERSION") }, "capabilities": { "experimentalApi": true } } }).to_string()
    }

    pub fn initialized(&self) -> String {
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }).to_string()
    }

    pub fn thread_start(&mut self, worktree: &Path, mode: Mode, resume: Option<&str>) -> String {
        let id = self.id();
        let (policy, sandbox) = match mode {
            Mode::Suggest => ("never", "read-only"),
            Mode::Edit => ("untrusted", "workspace-write"),
            Mode::Developer => ("untrusted", "workspace-write"),
        };
        let mut params = json!({ "cwd": worktree, "approvalPolicy": policy, "sandbox": sandbox });
        let method = match resume {
            Some(t) => {
                params["threadId"] = json!(t);
                "thread/resume"
            }
            None => "thread/start",
        };
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
    }

    pub fn turn_start(&mut self, text: &str) -> Option<String> {
        let thread = self.thread_id.clone()?;
        let id = self.id();
        Some(json!({ "jsonrpc": "2.0", "id": id, "method": "turn/start", "params": { "threadId": thread, "input": [{ "type": "text", "text": text }] } }).to_string())
    }

    pub fn turn_interrupt(&mut self) -> Option<String> {
        let (thread, turn) = (self.thread_id.clone()?, self.turn_id.clone()?);
        let id = self.id();
        Some(json!({ "jsonrpc": "2.0", "id": id, "method": "turn/interrupt", "params": { "threadId": thread, "turnId": turn } }).to_string())
    }

    /// Answer a permission request by its JSON-RPC id.
    pub fn approval_response(&mut self, request_id: &str, allow: bool) -> String {
        let item = self.pending.remove(request_id);
        let is_command = item
            .as_ref()
            .and_then(|i| self.items.get(i))
            .map(|(k, _, _)| *k == ToolKind::Shell)
            .unwrap_or(false);
        let decision = match (allow, is_command) {
            (true, _) => "accept",
            (false, _) => "decline",
        };
        let id: Value =
            serde_json::from_str(request_id).unwrap_or(Value::String(request_id.to_string()));
        json!({ "jsonrpc": "2.0", "id": id, "result": { "decision": decision } }).to_string()
    }

    /// Translates one stdout line, updating state.
    pub fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let Ok(m) = serde_json::from_str::<Value>(line) else {
            return vec![];
        };
        let mut out = Vec::new();
        // Responses to our requests: thread/start result carries the thread id.
        if m.get("method").is_none() {
            if let Some(thread) = m.pointer("/result/thread/id").and_then(Value::as_str) {
                self.thread_id = Some(thread.into());
                out.push(AgentEvent::SessionStarted {
                    session_id: thread.into(),
                });
            }
            if let Some(e) = m.get("error") {
                out.push(AgentEvent::Error {
                    message: e
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("codex error")
                        .into(),
                });
            }
            return out;
        }
        let method = m.get("method").and_then(Value::as_str).unwrap_or("");
        let params = m.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "turn/started" => {
                self.turn_id = params
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
            "item/agentMessage/delta" => {
                if let Some(d) = params.get("delta").and_then(Value::as_str) {
                    out.push(AgentEvent::TextDelta { text: d.into() });
                }
            }
            "item/started" => {
                let item = params.get("item").cloned().unwrap_or(Value::Null);
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                match item.get("type").and_then(Value::as_str) {
                    Some("fileChange") => {
                        let paths: Vec<String> = item
                            .get("changes")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(|c| {
                                        c.get("path").and_then(Value::as_str).map(str::to_string)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        self.items
                            .insert(id.clone(), (ToolKind::Write, paths.clone(), None));
                        out.push(AgentEvent::ToolCallStarted {
                            id,
                            kind: ToolKind::Write,
                            paths,
                            command: None,
                        });
                    }
                    Some("commandExecution") => {
                        let cmd = item
                            .get("command")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        self.items
                            .insert(id.clone(), (ToolKind::Shell, vec![], cmd.clone()));
                        out.push(AgentEvent::ToolCallStarted {
                            id,
                            kind: ToolKind::Shell,
                            paths: vec![],
                            command: cmd,
                        });
                    }
                    Some("mcpToolCall") | Some("webSearch") => {
                        let name = item
                            .get("tool")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string();
                        self.items
                            .insert(id.clone(), (ToolKind::Other(name.clone()), vec![], None));
                        out.push(AgentEvent::ToolCallStarted {
                            id,
                            kind: ToolKind::Other(name),
                            paths: vec![],
                            command: None,
                        });
                    }
                    _ => {}
                }
            }
            "item/completed" => {
                let item = params.get("item").cloned().unwrap_or(Value::Null);
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                match item.get("type").and_then(Value::as_str) {
                    Some("fileChange") => {
                        let status = item.get("status").and_then(Value::as_str).unwrap_or("");
                        let ok = status == "completed" || status == "applied" || status.is_empty();
                        out.push(AgentEvent::ToolCallFinished {
                            id,
                            ok,
                            summary: status.into(),
                        });
                    }
                    Some("commandExecution") => {
                        let status = item.get("status").and_then(Value::as_str).unwrap_or("");
                        let ok = status == "completed"
                            && item.get("exitCode").and_then(Value::as_i64).unwrap_or(0) == 0;
                        out.push(AgentEvent::ToolCallFinished {
                            id,
                            ok,
                            summary: item
                                .get("aggregatedOutput")
                                .and_then(Value::as_str)
                                .unwrap_or(status)
                                .chars()
                                .take(200)
                                .collect(),
                        });
                    }
                    Some("agentMessage") => {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            out.push(AgentEvent::Text { text: text.into() });
                        }
                    }
                    _ => {}
                }
            }
            "item/fileChange/requestApproval" | "item/commandExecution/requestApproval" => {
                let rpc_id = m.get("id").map(|v| v.to_string()).unwrap_or_default();
                let item_id = params
                    .get("itemId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let (kind, paths, command) =
                    self.items.get(&item_id).cloned().unwrap_or_else(|| {
                        if method.contains("command") {
                            (
                                ToolKind::Shell,
                                vec![],
                                params
                                    .get("command")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                            )
                        } else {
                            (ToolKind::Write, vec![], None)
                        }
                    });
                self.pending.insert(rpc_id.clone(), item_id);
                out.push(AgentEvent::PermissionRequest {
                    id: rpc_id,
                    kind,
                    paths,
                    command,
                });
            }
            "turn/completed" => {
                if let Some(e) = params.pointer("/turn/error").filter(|e| !e.is_null()) {
                    out.push(AgentEvent::Error {
                        message: e
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("turn failed")
                            .into(),
                    });
                }
                out.push(AgentEvent::TurnDone);
            }
            "error" => out.push(AgentEvent::Error {
                message: params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("codex error")
                    .into(),
            }),
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_the_deny_fixture() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/agents/codex-app-server-deny.jsonl"
        ))
        .unwrap();
        let mut st = CodexState::default();
        let events: Vec<AgentEvent> = text.lines().flat_map(|l| st.parse_line(l)).collect();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::SessionStarted { .. })));
        let reqs: Vec<&AgentEvent> = events
            .iter()
            .filter(|e| matches!(e, AgentEvent::PermissionRequest { .. }))
            .collect();
        assert_eq!(reqs.len(), 5, "4 file changes + 1 command");
        if let AgentEvent::PermissionRequest { kind, paths, .. } = reqs[0] {
            assert_eq!(*kind, ToolKind::Write);
            assert_eq!(
                paths,
                &vec!["/kb/notes.md".to_string(), "/kb/scratch.txt".to_string()]
            );
        }
        assert!(matches!(
            reqs[3],
            AgentEvent::PermissionRequest {
                kind: ToolKind::Shell,
                command: Some(_),
                ..
            }
        ));
        assert!(
            events
                .iter()
                .filter(|e| matches!(e, AgentEvent::TextDelta { .. }))
                .count()
                > 10
        );
        assert!(matches!(events.last(), Some(AgentEvent::TurnDone)));
        // approval reply uses the JSON-RPC id and decision
        let r: Value = serde_json::from_str(&st.approval_response("0", false)).unwrap();
        assert_eq!(r["id"], json!(0));
        assert_eq!(r["result"]["decision"], json!("decline"));
    }

    #[test]
    fn turn_start_needs_the_thread_id() {
        let mut st = CodexState::default();
        assert!(st.turn_start("hi").is_none(), "no thread yet");
        let ev = st.parse_line(r#"{"id":2,"result":{"thread":{"id":"t-1"}}}"#);
        assert!(matches!(
            ev.first(),
            Some(AgentEvent::SessionStarted { .. })
        ));
        let line = st.turn_start("hi").expect("thread id known");
        assert!(line.contains(r#""threadId":"t-1""#));
    }

    #[test]
    fn handshake_and_turn() {
        let mut st = CodexState::default();
        let init: Value = serde_json::from_str(&st.initialize()).unwrap();
        assert_eq!(init["method"], "initialize");
        let start: Value =
            serde_json::from_str(&st.thread_start(Path::new("/kb"), Mode::Suggest, None)).unwrap();
        assert_eq!(start["params"]["sandbox"], "read-only");
        assert_eq!(start["params"]["approvalPolicy"], "never");
        assert!(st.turn_start("hi").is_none(), "no thread yet");
        st.parse_line(r#"{"id":2,"result":{"thread":{"id":"t1"}}}"#);
        let turn: Value = serde_json::from_str(&st.turn_start("hi").unwrap()).unwrap();
        assert_eq!(turn["params"]["threadId"], "t1");
        let resume: Value =
            serde_json::from_str(&st.thread_start(Path::new("/kb"), Mode::Edit, Some("t1")))
                .unwrap();
        assert_eq!(resume["method"], "thread/resume");
    }
}
