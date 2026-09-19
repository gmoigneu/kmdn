# Spike 3: native agent adapters

Ran 2026-09-19 on Linux against the CLIs installed on the dev box: Claude Code 2.1.277, codex-cli 0.146.1, pi 0.83.0. Each agent used the user's existing login. No API keys.

Prompt for every agent: create `notes.md` and `scratch.txt`. Expected: `notes.md` lands, `scratch.txt` is blocked before it touches disk.

## Results

| Criterion | Claude Code | Codex | pi |
|---|---|---|---|
| Starts with existing login | pass | pass (ChatGPT login) | pass |
| Streams a reply as structured JSON | pass, `assistant` messages with content blocks | pass, `item/agentMessage/delta` | pass, `message_update` |
| Non-markdown write denied before disk | pass, `Edit(//<kb>/**/*.txt)` deny rule; tool_result reports denial; `permission_denials` listed in the final result | pass, `item/fileChange/requestApproval` answered `decline`; the patch is never applied | pass, `tool_call` handler in a kmdn extension returns `block` |
| Markdown write allowed | pass | pass, `accept` | pass, after a `confirm` round-trip to the host |
| Shell gated | pass, `Bash` in disallowedTools removes the tool | pass, `item/commandExecution/requestApproval` | pass, extension blocks `bash` |
| Resume after process restart | pass, `--resume <session_id>` | pass, `thread/resume` with the thread id | pass, `--session-dir` plus `--session-id` |
| Fixture recorded | `fixtures/claude-stream-json-deny.jsonl` | `fixtures/codex-app-server-deny.jsonl` | `fixtures/pi-rpc-deny.jsonl` |

## Transport details worth keeping

**Claude Code**: `claude -p --output-format stream-json --verbose --permission-mode acceptEdits --disallowedTools <rules...>` with the prompt on stdin. The `--disallowedTools` flag is variadic, so the prompt must come from stdin or precede the flags. Path rules use `Edit(//absolute/**/*.ext)` with a double slash for absolute paths. For the real adapter use `--input-format stream-json` for multi-turn plus `--permission-prompts host` so every non-auto-approved call reaches kmdn as a control request; deny rules stay as a second layer that holds even in bypass mode. Session id is in the `system/init` event. `--include-partial-messages` gives token-level deltas.

**Codex**: `codex app-server` on stdio, JSON-RPC 2.0. Handshake: `initialize` (with `capabilities.experimentalApi: true`) then `initialized`. `thread/start {cwd, approvalPolicy: "untrusted", sandbox: "workspace-write"}` then `turn/start {threadId, input: [{type: "text", text}]}`. Paths for a pending file change arrive in `item/started` with `item.type == "fileChange"` and `item.changes[].path`; the approval request `item/fileChange/requestApproval` carries only `itemId`, so the adapter keeps a map from itemId to paths. Reply `{decision: "accept" | "acceptForSession" | "decline"}`. `turn/completed` ends the turn. `turn/diff/updated` gives a running unified diff for free. Schemas: `codex app-server generate-json-schema --out <dir>`. Startup logs MCP transport errors to stderr from the user's own MCP config; harmless, keep stderr out of the UI.

**pi**: `pi --mode rpc --session-dir <dir> --session-id <uuid> -e <gate.ts>`. JSONL on stdin and stdout, split on `\n` only. `{"type":"prompt","message":...}` starts a turn; `agent_end` ends it. pi has no built-in permission prompt in RPC mode. kmdn ships a small extension (`pi_gate.ts` here) that handles `tool_call`: block writes outside allowed globs, block `bash` in Edit mode, and optionally call `ctx.ui.confirm`, which in RPC mode becomes an `extension_ui_request` on stdout that kmdn answers with `extension_ui_response`. Tool names are lowercase (`write`, `edit`, `bash`), path field is `path`. Closing stdin ends the process, so the adapter must keep stdin open until `agent_end` and then send `get_state` or exit explicitly.

## Decisions taken from this spike

1. The internal adapter interface in `docs/06-agents.md` holds. Add one field: `permission_request` carries `paths: Vec<PathBuf>` resolved by the adapter, since Codex sends them in a different message than the request.
2. pi permission gating is done through a kmdn-shipped extension, not a filesystem watcher. The watcher stays as a backstop for all agents.
3. Claude: use host-answered permission prompts as primary gate, deny rules as belt and braces. Suggest mode maps to `--permission-mode plan`.
4. Codex: Suggest mode maps to `sandbox: "read-only"`, `approvalPolicy: "never"`.
5. Fixtures in `fixtures/` are the seed for the adapter replay tests. Scratch paths were rewritten to `/kb`, account and rate-limit notifications removed.

## Not verified here
- macOS behavior. Same binaries, same protocols, but untested.
- Long sessions, compaction events, and usage-limit errors mid-turn.
- Image attachments over each protocol.
