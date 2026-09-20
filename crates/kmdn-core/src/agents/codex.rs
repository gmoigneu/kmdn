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

/// Splits a command line into words, honouring single and double quotes and backslashes.
/// Enough for the argv Codex reports; it is not a shell.
fn shell_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('\'') => cur.push(c),
            Some(_) => {
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        cur.push(n);
                    }
                } else {
                    cur.push(c);
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    in_word = true;
                }
                '\\' => {
                    if let Some(n) = chars.next() {
                        cur.push(n);
                        in_word = true;
                    }
                }
                c if c.is_whitespace() => {
                    if in_word {
                        out.push(std::mem::take(&mut cur));
                        in_word = false;
                    }
                }
                _ => {
                    cur.push(c);
                    in_word = true;
                }
            },
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

fn basename(w: &str) -> &str {
    w.rsplit('/').next().unwrap_or(w)
}

/// Commands that only read. No pagers, no filters that take an output file, nothing that can
/// run other programs. Redirections and pipes are rejected before we get here.
const READ_ONLY: &[&str] = &[
    "cat", "head", "tail", "wc", "ls", "rg", "grep", "egrep", "fgrep", "stat", "file", "du", "pwd",
    "echo", "printf", "basename", "dirname", "realpath", "which", "nl", "cut", "date", "whoami",
    "uname", "read", "test",
];

fn git_read_only(args: &[String]) -> bool {
    let mut it = args.iter();
    let mut sub = None;
    while let Some(a) = it.next() {
        if a == "-C" || a == "--git-dir" || a == "--work-tree" {
            it.next();
        } else if !a.starts_with('-') {
            sub = Some(a.as_str());
            break;
        }
    }
    let rest: Vec<&String> = it.collect();
    match sub {
        Some(
            "status" | "log" | "diff" | "show" | "rev-parse" | "ls-files" | "ls-tree" | "blame"
            | "grep" | "describe" | "shortlog" | "cat-file",
        ) => !rest.iter().any(|a| a.starts_with("--output")),
        Some("branch") => rest.iter().all(|a| {
            matches!(
                a.as_str(),
                "--show-current" | "--list" | "-a" | "-r" | "-v" | "-vv" | "--all"
            )
        }),
        Some("tag") => rest
            .iter()
            .all(|a| matches!(a.as_str(), "--list" | "-l" | "-n")),
        Some("remote") => rest.iter().all(|a| a.as_str() == "-v"),
        _ => false,
    }
}

/// `sed -n '<range>p' file...` only: `-e`, `-f`, `-i`, and any script that is not a plain
/// print range are refused, because sed scripts can write files.
fn sed_read_only(args: &[String]) -> bool {
    if !args.iter().any(|a| a == "-n") || args.iter().any(|a| a.starts_with('-') && a != "-n") {
        return false;
    }
    args.iter()
        .find(|a| a.as_str() != "-n")
        .map(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || ",$p;".contains(c)))
        .unwrap_or(false)
}

/// Read-only shell commands count as reads (D15 keeps real shell behind Developer mode).
/// Codex has no separate read tool: `cat`, `rg`, `git status` and friends are how it looks at
/// documents, and refusing them left it unable to work in Edit mode. Anything with pipes,
/// redirections, substitutions, several commands, or a tool outside a short allowlist stays
/// `Shell`. Transparent wrappers (`sh -c`, `command`, `rtk proxy`) are unwrapped first.
pub fn command_kind(cmd: &str) -> ToolKind {
    let meta = |s: &str| s.chars().any(|c| "|;&<>`$\n".contains(c));
    let mut words = shell_words(cmd);
    let is_shell = |w: &str| matches!(basename(w), "sh" | "bash" | "zsh" | "dash");
    if words.len() == 3
        && is_shell(&words[0])
        && words[1].starts_with('-')
        && words[1].contains('c')
        && words[1][1..].chars().all(|c| c == 'l' || c == 'c')
    {
        if meta(&words[2]) {
            return ToolKind::Shell;
        }
        words = shell_words(&words[2]);
    } else if meta(cmd) {
        return ToolKind::Shell;
    }
    loop {
        match words.first().map(|w| basename(w)) {
            Some("command") | Some("exec") => {
                words.remove(0);
            }
            Some("rtk") if words.get(1).map(String::as_str) == Some("proxy") => {
                words.drain(0..2);
            }
            Some("rtk") => {
                words.remove(0);
            }
            _ => break,
        }
    }
    let Some(first) = words.first() else {
        return ToolKind::Shell;
    };
    let args = &words[1..];
    let read_only = match basename(first) {
        "git" => git_read_only(args),
        "sed" => sed_read_only(args),
        "find" => !args.iter().any(|a| {
            matches!(
                a.as_str(),
                "-delete"
                    | "-exec"
                    | "-execdir"
                    | "-ok"
                    | "-okdir"
                    | "-fprint"
                    | "-fprintf"
                    | "-fls"
            )
        }),
        "rg" => !args.iter().any(|a| a.starts_with("--pre")),
        n => READ_ONLY.contains(&n),
    };
    if read_only {
        ToolKind::Read
    } else {
        ToolKind::Shell
    }
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
                        let kind = cmd.as_deref().map(command_kind).unwrap_or(ToolKind::Shell);
                        self.items
                            .insert(id.clone(), (kind.clone(), vec![], cmd.clone()));
                        out.push(AgentEvent::ToolCallStarted {
                            id,
                            kind,
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
                            let cmd = params
                                .get("command")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            let kind = cmd.as_deref().map(command_kind).unwrap_or(ToolKind::Shell);
                            (kind, vec![], cmd)
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
    fn read_only_commands_are_reads_everything_else_is_shell() {
        let read = [
            "/bin/zsh -lc 'cat engineering/deploy.md'",
            "/bin/zsh -lc 'rtk proxy cat engineering/deploy.md'",
            "/bin/zsh -lc 'rtk read engineering/deploy.md'",
            "bash -lc \"rg -n 'rollback' .\"",
            "/bin/zsh -lc 'sed -n 1,80p README.md'",
            "/bin/zsh -lc 'git status --short'",
            "/bin/zsh -lc 'git -C . log --oneline -5'",
            "/bin/zsh -lc 'find . -name AGENTS.md -print'",
            "ls -la engineering",
            "/bin/zsh -lc 'head -n 20 people/time-off.md'",
        ];
        for c in read {
            assert_eq!(command_kind(c), ToolKind::Read, "{c}");
        }
        let shell = [
            "/bin/zsh -lc 'cat a.md > b.md'",
            "/bin/zsh -lc 'cat a.md | grep x'",
            "/bin/zsh -lc 'rm -rf engineering'",
            "/bin/zsh -lc 'find . -name \"*.md\" -delete'",
            "/bin/zsh -lc 'sed -i s/a/b/ README.md'",
            "/bin/zsh -lc 'sed -n \"s/a/b/w out\" README.md'",
            "/bin/zsh -lc 'git push origin main'",
            "/bin/zsh -lc 'git branch -D main'",
            "/bin/zsh -lc 'git diff --output=x.patch'",
            "/bin/zsh -lc 'python3 -c print(1)'",
            "/bin/zsh -lc 'rg --pre cat foo'",
            "/bin/zsh -lc 'echo $(whoami)'",
            "/bin/zsh -lc 'cat a.md; rm a.md'",
            "/bin/zsh -lc 'rtk gain'",
            "",
        ];
        for c in shell {
            assert_eq!(command_kind(c), ToolKind::Shell, "{c}");
        }
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
