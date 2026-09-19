//! pi adapter: `pi --mode rpc -e <gate>`, JSONL over stdio. Verified in spikes/agents/README.md.
//! pi has no built-in permission prompt in RPC mode; kmdn ships an extension (pi_gate.ts) that
//! blocks disallowed writes itself and asks the host through `extension_ui_request` confirm.

use std::path::Path;

use serde_json::{json, Value};

use super::{AgentEvent, Mode, ToolKind};

/// The gate extension source, written next to the worktree at launch.
pub const GATE_EXTENSION: &str = include_str!("pi_gate.ts");

/// Writes the gate extension for the given mode and returns its path.
pub fn write_gate(dir: &Path, mode: Mode) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("kmdn-gate.ts");
    let src = GATE_EXTENSION.replace(
        "const MODE = \"edit\";",
        &format!(
            "const MODE = \"{}\";",
            match mode {
                Mode::Suggest => "suggest",
                Mode::Edit => "edit",
                Mode::Developer => "developer",
            }
        ),
    );
    std::fs::write(&path, src)?;
    Ok(path)
}

pub fn args(gate: &Path, session_dir: &Path, session_id: Option<&str>) -> Vec<String> {
    let mut a = vec![
        "--mode".to_string(),
        "rpc".into(),
        "-e".into(),
        gate.to_string_lossy().to_string(),
        "--session-dir".into(),
        session_dir.to_string_lossy().to_string(),
    ];
    if let Some(id) = session_id {
        a.push("--session-id".into());
        a.push(id.into());
    }
    a
}

pub fn prompt(text: &str, id: &str) -> String {
    json!({ "id": id, "type": "prompt", "message": text }).to_string()
}

pub fn abort() -> String {
    json!({ "type": "abort" }).to_string()
}

/// Reply to an `extension_ui_request` confirm.
pub fn ui_response(id: &str, confirmed: bool) -> String {
    json!({ "type": "extension_ui_response", "id": id, "confirmed": confirmed, "value": confirmed })
        .to_string()
}

fn tool_kind(name: &str) -> ToolKind {
    match name {
        "read" | "ls" | "grep" | "find" | "glob" => ToolKind::Read,
        "write" | "edit" => ToolKind::Write,
        "bash" => ToolKind::Shell,
        other => ToolKind::Other(other.into()),
    }
}

pub fn parse_line(line: &str) -> Vec<AgentEvent> {
    let Ok(m) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    let t = m.get("type").and_then(Value::as_str).unwrap_or("");
    let mut out = Vec::new();
    match t {
        "response" => {
            if m.get("success").and_then(Value::as_bool) == Some(false) {
                out.push(AgentEvent::Error {
                    message: m
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("pi rejected the command")
                        .into(),
                });
            }
            if m.get("command").and_then(Value::as_str) == Some("get_state") {
                if let Some(id) = m.pointer("/data/sessionId").and_then(Value::as_str) {
                    out.push(AgentEvent::SessionStarted {
                        session_id: id.into(),
                    });
                }
            }
        }
        "message_update" => {
            let ev = m
                .get("assistantMessageEvent")
                .cloned()
                .unwrap_or(Value::Null);
            if ev.get("type").and_then(Value::as_str) == Some("text_delta") {
                if let Some(d) = ev.get("delta").and_then(Value::as_str) {
                    out.push(AgentEvent::TextDelta { text: d.into() });
                }
            }
        }
        "tool_execution_start" => {
            let name = m.get("toolName").and_then(Value::as_str).unwrap_or("");
            let args = m.get("args").cloned().unwrap_or(Value::Null);
            let paths = args
                .get("path")
                .and_then(Value::as_str)
                .map(|p| vec![p.to_string()])
                .unwrap_or_default();
            out.push(AgentEvent::ToolCallStarted {
                id: m
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                kind: tool_kind(name),
                paths,
                command: args
                    .get("command")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
        "tool_execution_end" => {
            let content = m
                .pointer("/result/content")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            out.push(AgentEvent::ToolCallFinished {
                id: m
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                ok: !m.get("isError").and_then(Value::as_bool).unwrap_or(false),
                summary: content.chars().take(200).collect(),
            });
        }
        "extension_ui_request" if m.get("method").and_then(Value::as_str) == Some("confirm") => {
            // The gate asks "Allow write to <path>?"; recover the path from the message.
            let msg = m.get("message").and_then(Value::as_str).unwrap_or("");
            let path = msg
                .strip_prefix("Allow write to ")
                .and_then(|s| s.strip_suffix('?'))
                .map(str::to_string);
            let is_shell = msg.starts_with("Allow command");
            out.push(AgentEvent::PermissionRequest {
                id: m.get("id").and_then(Value::as_str).unwrap_or("").into(),
                kind: if is_shell {
                    ToolKind::Shell
                } else {
                    ToolKind::Write
                },
                paths: path.into_iter().collect(),
                command: if is_shell {
                    msg.strip_prefix("Allow command: ").map(str::to_string)
                } else {
                    None
                },
            });
        }
        "extension_error" | "error" => out.push(AgentEvent::Error {
            message: m
                .get("error")
                .or(m.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("pi error")
                .into(),
        }),
        "agent_end" => out.push(AgentEvent::TurnDone),
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
            "/tests/fixtures/agents/pi-rpc-deny.jsonl"
        ))
        .unwrap();
        let events: Vec<AgentEvent> = text.lines().flat_map(parse_line).collect();
        let starts: Vec<&AgentEvent> = events
            .iter()
            .filter(|e| matches!(e, AgentEvent::ToolCallStarted { .. }))
            .collect();
        assert_eq!(starts.len(), 2);
        assert!(
            matches!(starts[0], AgentEvent::ToolCallStarted { kind: ToolKind::Write, paths, .. } if paths == &vec!["notes.md".to_string()])
        );
        let perm = events
            .iter()
            .find(|e| matches!(e, AgentEvent::PermissionRequest { .. }))
            .unwrap();
        assert!(
            matches!(perm, AgentEvent::PermissionRequest { paths, .. } if paths == &vec!["notes.md".to_string()])
        );
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
            "scratch.txt blocked by the gate"
        );
        assert!(matches!(events.last(), Some(AgentEvent::TurnDone)));
    }

    #[test]
    fn gate_is_written_with_the_mode_and_args_are_right() {
        let d = tempfile::tempdir().unwrap();
        let gate = write_gate(d.path(), Mode::Suggest).unwrap();
        let src = std::fs::read_to_string(&gate).unwrap();
        assert!(src.contains("const MODE = \"suggest\";"));
        let a = args(&gate, d.path(), Some("s1"));
        assert_eq!(&a[..2], &["--mode".to_string(), "rpc".to_string()]);
        assert!(a.contains(&"--session-id".to_string()));
        let r: Value = serde_json::from_str(&ui_response("u1", false)).unwrap();
        assert_eq!(r["confirmed"], false);
    }
}
