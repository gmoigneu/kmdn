//! Claude Code adapter: `claude -p --input-format stream-json --output-format stream-json`.
//! Newline-delimited JSON. Verified in spikes/agents/README.md.

use std::path::Path;

use serde_json::{json, Value};

use super::{AgentEvent, Mode, ToolKind};

/// Arguments after the binary. Prompt and follow-ups are written to stdin as stream-json
/// user messages, so `--disallowedTools` being variadic is not a problem.
pub fn args(mode: Mode, worktree: &Path, resume: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-p".into(),
        "--verbose".into(),
        "--input-format".into(),
        "stream-json".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--include-partial-messages".into(),
        "--permission-prompts".into(),
        "host".into(),
    ];
    a.push("--permission-mode".into());
    a.push(
        match mode {
            Mode::Suggest => "plan",
            Mode::Edit => "default",
            Mode::Developer => "default",
        }
        .into(),
    );
    // Belt and braces: deny rules hold even if a later mode change auto-approves edits (D15).
    let wt = worktree.to_string_lossy();
    a.push("--disallowedTools".into());
    for ext in [
        "txt", "json", "yaml", "yml", "toml", "js", "ts", "py", "rs", "sh", "html", "css",
    ] {
        a.push(format!("Edit(//{wt}/**/*.{ext})"));
    }
    a.push(format!("Edit(//{wt}/AGENTS.md)"));
    a.push(format!("Edit(//{wt}/.kmdn/**)"));
    if mode != Mode::Developer {
        a.push("Bash".into());
    }
    if let Some(id) = resume {
        a.push("--resume".into());
        a.push(id.into());
    }
    a
}

/// A user turn on stdin.
pub fn user_message(text: &str) -> String {
    json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "text", "text": text }] } }).to_string()
}

/// Reply to a `control_request` permission prompt.
pub fn permission_response(
    request_id: &str,
    allow: bool,
    reason: &str,
    updated_input: Option<Value>,
) -> String {
    let response = if allow {
        json!({ "behavior": "allow", "updatedInput": updated_input.unwrap_or(json!({})) })
    } else {
        json!({ "behavior": "deny", "message": reason })
    };
    json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } }).to_string()
}

fn tool_kind(name: &str) -> ToolKind {
    match name {
        "Read" | "Glob" | "Grep" | "LS" | "WebFetch" | "WebSearch" | "NotebookRead" => {
            ToolKind::Read
        }
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => ToolKind::Write,
        "Bash" | "PowerShell" => ToolKind::Shell,
        other => ToolKind::Other(other.to_string()),
    }
}

fn paths_of(name: &str, input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for k in ["file_path", "path", "notebook_path"] {
        if let Some(p) = input.get(k).and_then(Value::as_str) {
            out.push(p.to_string());
        }
    }
    if name == "MultiEdit" {
        if let Some(edits) = input.get("edits").and_then(Value::as_array) {
            for e in edits {
                if let Some(p) = e.get("file_path").and_then(Value::as_str) {
                    out.push(p.to_string());
                }
            }
        }
    }
    out
}

/// Translates one stdout line. Unknown lines produce no events.
pub fn parse_line(line: &str) -> Vec<AgentEvent> {
    let Ok(m) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    let t = m.get("type").and_then(Value::as_str).unwrap_or("");
    let mut out = Vec::new();
    match t {
        "system" if m.get("subtype").and_then(Value::as_str) == Some("init") => {
            if let Some(id) = m.get("session_id").and_then(Value::as_str) {
                out.push(AgentEvent::SessionStarted {
                    session_id: id.into(),
                });
            }
        }
        "stream_event" => {
            // Partial messages: text deltas.
            if let Some(delta) = m.pointer("/event/delta") {
                if delta.get("type").and_then(Value::as_str) == Some("text_delta") {
                    if let Some(text) = delta.get("text").and_then(Value::as_str) {
                        out.push(AgentEvent::TextDelta { text: text.into() });
                    }
                }
            }
        }
        "assistant" => {
            for c in m
                .pointer("/message/content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                match c.get("type").and_then(Value::as_str) {
                    Some("tool_use") => {
                        let name = c.get("name").and_then(Value::as_str).unwrap_or("");
                        let input = c.get("input").cloned().unwrap_or(Value::Null);
                        out.push(AgentEvent::ToolCallStarted {
                            id: c.get("id").and_then(Value::as_str).unwrap_or("").into(),
                            kind: tool_kind(name),
                            paths: paths_of(name, &input),
                            command: input
                                .get("command")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        });
                    }
                    Some("text") => {
                        // Full text arrives here too; deltas already streamed it when partials are on.
                        if let Some(text) = c.get("text").and_then(Value::as_str) {
                            out.push(AgentEvent::Text { text: text.into() });
                        }
                    }
                    _ => {}
                }
            }
        }
        "user" => {
            for c in m
                .pointer("/message/content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                if c.get("type").and_then(Value::as_str) == Some("tool_result") {
                    let content = match c.get("content") {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Array(a)) => a
                            .iter()
                            .filter_map(|x| x.get("text").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        _ => String::new(),
                    };
                    let is_error = c.get("is_error").and_then(Value::as_bool).unwrap_or(false)
                        || content.contains("<tool_use_error>");
                    out.push(AgentEvent::ToolCallFinished {
                        id: c
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .into(),
                        ok: !is_error,
                        summary: content.chars().take(200).collect(),
                    });
                }
            }
        }
        "control_request" => {
            // Host-answered permission prompt.
            let req = m.get("request").cloned().unwrap_or(Value::Null);
            if req.get("subtype").and_then(Value::as_str) == Some("can_use_tool") {
                let name = req.get("tool_name").and_then(Value::as_str).unwrap_or("");
                let input = req.get("input").cloned().unwrap_or(Value::Null);
                out.push(AgentEvent::PermissionRequest {
                    id: m
                        .get("request_id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    kind: tool_kind(name),
                    paths: paths_of(name, &input),
                    command: input
                        .get("command")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
        }
        "result" => {
            if m.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
                out.push(AgentEvent::Error {
                    message: m
                        .get("result")
                        .and_then(Value::as_str)
                        .unwrap_or("agent error")
                        .into(),
                });
            }
            out.push(AgentEvent::TurnDone);
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_the_deny_fixture() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/agents/claude-stream-json-deny.jsonl"
        ))
        .unwrap();
        let events: Vec<AgentEvent> = text.lines().flat_map(parse_line).collect();
        assert!(matches!(events[0], AgentEvent::SessionStarted { .. }));
        let writes: Vec<&AgentEvent> = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    AgentEvent::ToolCallStarted {
                        kind: ToolKind::Write,
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(writes.len(), 2);
        if let AgentEvent::ToolCallStarted { paths, .. } = writes[0] {
            assert_eq!(paths, &vec!["/kb/notes.md".to_string()]);
        }
        let finished: Vec<bool> = events
            .iter()
            .filter_map(|e| {
                if let AgentEvent::ToolCallFinished { ok, .. } = e {
                    Some(*ok)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            finished,
            vec![true, false],
            "second write (scratch.txt) was denied"
        );
        assert!(matches!(events.last(), Some(AgentEvent::TurnDone)));
    }

    #[test]
    fn args_and_messages() {
        let a = args(Mode::Edit, Path::new("/kb"), Some("sid"));
        assert!(
            a.contains(&"stream-json".to_string())
                && a.contains(&"Bash".to_string())
                && a.contains(&"--resume".to_string())
        );
        assert!(a.iter().any(|x| x == "Edit(///kb/**/*.txt)"));
        let dev = args(Mode::Developer, Path::new("/kb"), None);
        assert!(!dev.contains(&"Bash".to_string()));
        assert!(args(Mode::Suggest, Path::new("/kb"), None).contains(&"plan".to_string()));
        let u: Value = serde_json::from_str(&user_message("hi")).unwrap();
        assert_eq!(
            u.pointer("/message/content/0/text").and_then(Value::as_str),
            Some("hi")
        );
        let r: Value =
            serde_json::from_str(&permission_response("r1", false, "nope", None)).unwrap();
        assert_eq!(
            r.pointer("/response/response/behavior")
                .and_then(Value::as_str),
            Some("deny")
        );
    }

    #[test]
    fn parses_control_requests() {
        let line = r#"{"type":"control_request","request_id":"req-9","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/kb/x.txt","content":"a"}}}"#;
        let ev = parse_line(line);
        assert_eq!(
            ev,
            vec![AgentEvent::PermissionRequest {
                id: "req-9".into(),
                kind: ToolKind::Write,
                paths: vec!["/kb/x.txt".into()],
                command: None
            }]
        );
    }
}
