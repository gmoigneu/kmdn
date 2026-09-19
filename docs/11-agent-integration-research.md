# Agent integration research

Question: is ACP the right way to embed pi, Claude Code, and Codex in kmdn? Researched 2026-09-19 by looking at what shipping multi-agent apps do.

## What the wrappers actually do

| App | Stack | How it runs agents | Permissions | Notes |
|---|---|---|---|---|
| Orca (stablyai) | Electron | PTY terminal per agent, any CLI. Status from OSC title sequences plus Claude Code and Codex hooks | Launches with `--dangerously-skip-permissions` by default, "Yolo" vs "Manual" global toggle | 40+ agents because it does not parse anything. Diff review is git-based, not agent-based |
| Conductor | Tauri | Claude Agent SDK (TypeScript), bundled and pinned. Codex, OpenCode, Cursor SDKs bundled | SDK permission callbacks | Requires the user's Claude Code subscription. First-party protocols only |
| Nimbalyst (ex Crystal) | Electron | Claude CLI in `--output-format stream-json --input-format stream-json --permission-prompt-tool stdio`. Codex and OpenCode via their own structured modes | permission-prompt-tool over stdio | Rich markdown editor in the same app, closest to kmdn's shape |
| Vibe Kanban | Rust backend, React | `StandardCodingAgentExecutor` trait, one executor per agent spawning the CLI as a child process, output normalized into common log entries | Per-agent, normalized | Rust, so the same layer kmdn would write in Tauri |
| Superset | Electron | PTY terminals, plus SDK and MCP for automation | Agent's own | Status dashboard from terminal state |
| Zed, JetBrains | native | ACP client, agents via ACP adapters | ACP permission requests | The only category using ACP as the core |

Pattern: **no wrapper app uses ACP as its core**. Editors do, because they want any agent to plug in. Wrapper apps either give up structure (PTY) or use each vendor's official structured interface.

## The three agents, first-party options

**Claude Code**
- Official structured mode: `claude -p --input-format stream-json --output-format stream-json`, newline-delimited JSON over stdio. Gating: `--permission-prompt-tool`, `--disallowedTools "Edit(path pattern)"`, PreToolUse hooks. Deny rules hold even in bypass mode. Session resume supported.
- Agent SDK (TypeScript, Python) wraps the same engine. Not usable from Rust directly, would mean a Node sidecar.
- ACP: `@agentclientprotocol/claude-agent-acp`, maintained by the ACP org, built on the Agent SDK. Zed reports gaps: plan mode unavailable, some slash commands broken, session stuck after usage limit, context window not respected.
- **Billing risk**: Anthropic's SDK docs state third parties may not offer claude.ai login or subscription rate limits for products built on the Agent SDK without approval. On 2026-06-15 Anthropic announced a separate paid "Agent SDK credit" pool for ACP and SDK use, then paused it the next day and said the plan is being revised. Zed's workaround was: run the official `claude` CLI. Spawning the user's own installed CLI in stream-json mode is what Nimbalyst does and what Conductor's subscription requirement implies is tolerated. It is not explicitly blessed. Track it.

**Codex**
- Official structured mode: `codex app-server`, JSON-RPC 2.0 over stdio, also WebSocket. Threads, turns, items, streaming deltas, approval requests. This is what the Codex VS Code extension and the Codex desktop app use. Core methods stable, marked experimental overall. ChatGPT subscription login works locally.
- `codex exec --json`: one-shot, no approvals, for CI.
- ACP: `@agentclientprotocol/codex-acp`, wraps the CLI. Repo moved in July 2026.

**pi**
- Official structured mode: `pi --mode rpc`, pi's own JSON dialect over stdio.
- ACP: five or more community forks of `pi-acp`, each a bridge over `--mode rpc`. The pi maintainers' discussion lists their limits: no permission request pathway to the host, no filesystem delegation, MCP pass-through ignored, version skew with pi's RPC. Native ACP in pi is proposed, not decided.

## ACP as core: verdict

For kmdn:
- Claude Code and pi would both go through community bridges, each with known holes, and the Claude bridge sits on the wrong side of a billing rule that is still moving.
- Permission gating, the one thing kmdn must get right for non-devs, is exactly where the pi bridge is weakest.
- ACP's strength, plugging in any of 50 agents, is not a kmdn goal. Three agents are.

ACP is the right answer for an editor. kmdn is a wrapper with three named agents.

## PTY as core: verdict

Orca proves it scales to any agent with near-zero per-agent work. It fails kmdn's requirements: no structured tool calls, no per-write permission gate, no clean accept-or-revert per file, status by heuristics. Non-devs would see a terminal.

## Recommendation: native adapters behind one internal interface

Copy Vibe Kanban's shape. One Rust trait, one adapter per agent, each speaking the vendor's official structured protocol:

| Agent | Adapter transport | Permission gate |
|---|---|---|
| Claude Code | user's `claude` CLI, stream-json in and out | `--permission-prompt-tool` plus `--disallowedTools` path rules for everything outside `**/*.md` and `assets/**` |
| Codex | user's `codex app-server` over stdio | approval requests on the JSON-RPC stream, kmdn answers |
| pi | user's `pi --mode rpc` | pi's RPC permission events, verified in the spike |
| Anything else, later | generic ACP adapter | ACP permission requests |

The internal interface normalizes: session start and resume, user message, streamed assistant text, tool call started and finished with file paths, permission request and reply, cancel, done, error. The UI timeline renders that and nothing agent-specific.

Why this wins:
- Users run their own installed CLIs with their own logins, the same way Orca, Nimbalyst, and Conductor work.
- Each transport is the one the vendor's own products use, so it is the best maintained.
- Three adapters is a bounded cost. Vibe Kanban maintains ten in Rust.
- ACP is not lost: it becomes the fourth adapter for long-tail agents, and if pi ships native ACP the pi adapter can switch.

Costs:
- Three wire formats to track. Mitigation: recorded-session fixtures per agent in CI, re-recorded on CLI updates.
- Claude subscription rules may tighten. Mitigation: the adapter takes an API key too, and the adapter boundary means a policy change is one file, not the app.

## Changes if adopted
- D14 superseded: ACP moves from core to fallback adapter.
- Spike 3 becomes: for each agent, drive its native structured mode from Rust, stream a reply, gate a file write outside allowed paths, resume a session.
- 06-agents.md: replace ACP wording with the adapter interface.

## Sources
- Orca: https://github.com/stablyai/orca, https://www.onorca.dev/docs/model/agents-sessions, https://www.onorca.dev/docs/agents/supported, https://www.onorca.dev/docs/agents/hooks-memory
- Conductor: https://www.conductor.build/docs/, https://github.com/deathemperor/infinitus/issues/270
- Nimbalyst: https://github.com/nimbalyst/nimbalyst, https://deepwiki.com/nimbalyst/nimbalyst
- Vibe Kanban: https://github.com/BloopAI/vibe-kanban, https://deepwiki.com/BloopAI/vibe-kanban
- Superset: https://superset.sh/
- ACP: https://agentclientprotocol.com/get-started/agents, https://casys.ai/blog/acpx-multi-agent-orchestration
- claude-agent-acp: https://github.com/zed-industries/claude-agent-acp
- codex-acp: https://github.com/zed-industries/codex-acp
- pi ACP discussion: https://github.com/earendil-works/pi/discussions/4444
- Codex app-server: https://learn.chatgpt.com/docs/app-server
- Claude Agent SDK overview and permissions: https://code.claude.com/docs/en/agent-sdk, https://code.claude.com/docs/en/agent-sdk/permissions
- Zed on Anthropic billing: https://zed.dev/blog/anthropic-subscription-changes
- Zed ACP issues: https://github.com/zed-industries/zed/issues/51648, https://github.com/zed-industries/zed/issues/55501
