# Agents

## Model

kmdn defines one internal agent interface in Rust and ships one adapter per agent. Each adapter speaks the vendor's official structured protocol to the CLI the user installed and logged into. Rationale and alternatives in [11-agent-integration-research.md](11-agent-integration-research.md).

| Agent | Transport | Permission gate |
|---|---|---|
| Claude Code | `claude -p --input-format stream-json --output-format stream-json`, newline JSON over stdio | `--permission-prompt-tool` answered by kmdn, `--disallowedTools "Edit(...)"` deny rules for paths outside allowed globs, PreToolUse hook as belt and braces |
| Codex | `codex app-server`, JSON-RPC 2.0 over stdio | approval requests on the stream, answered by kmdn |
| pi | `pi --mode rpc -e <kmdn gate extension>`, pi's JSONL over stdio | kmdn-shipped extension handles `tool_call`: blocks writes outside allowed globs, asks the host via `extension_ui_request` confirm |
| Others, later | generic ACP client | ACP `session/request_permission` |

### Internal interface, normalized events
- `start(cwd, mode, system_context) -> session`, `resume(session_id)`
- `send(user_message, attachments)`
- streamed out: `text_delta`, `tool_call_started {id, kind, paths}`, `tool_call_finished {id, result}`, `permission_request {id, kind, paths, command}` (paths resolved by the adapter; Codex sends them in `item/started`, not in the request), `turn_done`, `error`
- in: `permission_reply {id, allow|deny, reason}`, `cancel`
- `models()` when the agent exposes a list, else empty

The UI timeline renders only these events. Nothing agent-specific leaks above the adapter.

### Verified 2026-09-19
Spike 3 passed all criteria for the three agents: start with the user's login, stream, deny a non-markdown write before disk, allow a markdown write, gate shell, resume after restart. Details and fixtures in `spikes/agents/`.

### Wire-format drift
Each adapter has recorded-session fixtures in CI. When a CLI release changes its output, the fixture test fails before users notice. Re-record on upgrade.

### Claude subscription note
Anthropic restricts subscription use through the Agent SDK and ACP for third parties; the policy was announced and paused in June 2026. kmdn spawns the user's own CLI, as Nimbalyst and Conductor do. The Claude adapter also accepts an API key. If the policy tightens, the change is confined to that adapter.

## Launch

- Per agent a launcher config: binary name, args, env, detection (which, version, minimum version).
- Settings panel lists detected agents with install hints for missing ones.
- One agent session per thread, working directory is the thread's worktree. Several threads run agents in parallel.

## Context given to the agent

System-level instructions sent at session start:
- This is a markdown knowledge base. GFM plus frontmatter. Relative links. Read `AGENTS.md` first.
- Frontmatter schema summary.
- Only `*.md` and `assets/**` may be written. Do not touch `.kmdn/` or `AGENTS.md`.
- Currently open document path and selection, if any, plus the user's request.

## Permission gate

Every adapter surfaces tool calls as normalized permission requests. kmdn policy:

| Tool call | Policy |
|---|---|
| Read any file in repo | allow |
| Write or create under allowed paths | allow, tracked as pending agent edit |
| Write outside allowed paths | deny, explain to agent. Also enforced by the agent's own deny rules where supported, so a bypass mode cannot leak |
| Shell commands | ask, off by default for non-dev profile |
| Network | agent's own concern, kmdn does not gate |

Modes, chosen in the composer per message: **Suggest** (no writes, agent proposes text in the timeline), **Edit** (default, writes under allowed paths), **Developer** (shell allowed with per-call prompt, visible only when enabled in settings).

## Edits and review

- Agent edits land in the thread's worktree. A message sent from Home creates the thread first.
- Every agent-touched file shows in an "agent changes" list with a diff. The user accepts or reverts per file before the next auto-commit includes them. Reverting restores the pre-agent content.
- Accepted edits commit with message `Agent (<name>): <short request>`. Author is the human user. Trailer `Co-Authored-By` names the agent.
- Submit for review is always a human action.
- At submit, a condensed log of the thread's agent activity is posted to the PR as one collapsible comment, after the author previews it. Full transcripts stay in local SQLite.

## Agent-assisted submit

At submit, kmdn sends the change set diff to the default agent with a fixed prompt asking for a title under 70 characters and a three-line summary. Output is shown for editing before the PR is created. No file writes are permitted in this session. If no agent is installed or the call fails, the deterministic fallback is used.

## Failure handling

- Agent crash or non-zero exit: chat shows the error, pending edits remain and can be reverted.
- Agent writes a non-markdown file despite denial: watcher shows it as an untracked external file. Never committed.

## Not in v1

- Agents opening or reviewing PRs.
- Scheduled or background agent runs.
- Agents reading provider comments as context. Plausible next step.
