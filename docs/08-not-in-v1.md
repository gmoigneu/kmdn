# Not in v1

Decided out of scope on 2026-09-18. Each item names the hook that keeps the door open.

| Excluded | Why | Hook |
|---|---|---|
| Live multi-user editing (CRDT) | Needs a sync transport or server | Editor buffer is plain text, CRDT can wrap it later |
| Windows build | Subprocess and path quirks, agent CLIs less reliable | Tauri targets it, launchers are config |
| MCP server, semantic search | Generated index and grep suffice to start | Index service already has the data |
| Relay server, kmdn-native identity | Means running a service; provider accounts required | Provider trait gets a relay impl |
| Wikilinks, Mermaid, math, admonitions | Break provider rendering, confuse agents | Parser is pluggable |
| Non-image attachments | Repo bloat, unreviewable | Allowed paths are config |
| Commit signing | Complexity, keychain and GPG per platform | libgit2 supports it |
| Agents opening or reviewing PRs | Human owns submit and merge in v1 | Provider trait is available to the agent host |
| Multi-repo workspace | One KB at a time | Repo service is per path already |
| Curated navigation manifest | Folder tree is enough | `order` key exists |
| Per-user roles inside kmdn | Provider permissions are the roles | None needed |
| Telemetry and crash reporting | Trust first | Local log and copy-diagnostics exist |
| Idle auto-commit | Explicit save only | Setting can be added |
| Homebrew cask, Flatpak | Releases plus updater cover v1 | Same release pipeline |
| Agent assistance inside the review layout | Reviewers read first | Review drawer can host a composer |
| Multiple agent attempts per task (best of N) | One agent per thread | Threads are cheap, fork later |
| Scheduled or automated threads | Human starts every thread | Thread creation is an API in the core |
