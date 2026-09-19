# Architecture

## Stack

| Layer | Choice | Notes |
|---|---|---|
| Shell | Tauri 2 | Rust backend, system webview |
| UI | React, TypeScript, Vite | |
| Editor | CodeMirror 6 with live-preview decorations | See [07-editor.md](07-editor.md) |
| Git | git2-rs (libgit2, bundled) | Clone, fetch, commit, rebase, push |
| Providers | Rust trait `Provider`, impls for GitHub and GitLab | REST plus GraphQL where needed |
| Agents | Rust adapter per agent over the vendor's stdio protocol; ACP adapter for others | See [06-agents.md](06-agents.md) |
| Local state | SQLite via rusqlite, in app data dir | Search index, caches, UI prefs |
| Secrets | OS keychain via Tauri plugin | Provider tokens |
| File watch | notify crate | Detect external edits |

## Components

```
+---------------------------- Tauri app -----------------------------+
|  React UI (Tailwind, shadcn/ui)                                    |
|   home: composer + task list | sidebar: threads, reviews, docs     |
|   thread: timeline + right pane (Changes, Editor, Read)            |
|   review layout | palette | conflict resolver | notifications      |
|                        | Tauri commands / events                   |
|  Rust core                                                         |
|   repo service   : git2, main clone, worktree per thread, rebase   |
|   provider       : trait + GitHub + GitLab (custom base URL)       |
|   review service : PRs, rendered diff, comments, approve, merge    |
|   comments       : issues by doc path                              |
|   index service  : frontmatter scan, SQLite FTS, AGENTS.md gen     |
|   watcher        : fs events -> change set adoption                |
|   agent host     : adapters claude/codex/pi/acp, permission gate  |
|   auth           : device flow, PAT, keychain                      |
+--------------------------------------------------------------------+
        |                    |                          |
 main clone + worktrees  GitHub / GitLab         pi / claude / codex
 (user-chosen path)       REST APIs           (native stdio protocols,
                                                  cwd = worktree)
```

## Data flow: the golden path

1. Open KB: user picks or clones a repo to a local path. kmdn detects the provider from the remote URL, prompts for auth if needed.
2. Index: render the tree from the filesystem immediately. Scan `*.md`, parse frontmatter, build the SQLite FTS index in the background with a progress hint. Target: 5,000 documents, 1 GB clone. Incremental reindex on file events.
3. Start a thread: from the Home composer or Edit on a document. kmdn creates branch `kmdn/<user>/<slug>` and a worktree. Agent turns and manual edits happen in that worktree. Only explicit saves commit; accepted agent edits commit on save.
4. Save: writes file, auto-commits into the change set branch. Watcher-detected external edits are adopted the same way.
5. Submit: kmdn regenerates the index file, commits, pushes. The embedded agent drafts title and summary from the diff, the user confirms, kmdn opens the PR.
6. Review: reviewer opens the review panel, sees rendered word-level diff, comments inline, approves, merges via provider API.
7. Sync: kmdn fetches main, fast-forwards, re-indexes. Other open change sets rebase automatically.
8. Consume: any agent clones the repo and starts from the index file.

## Provider trait, minimum surface

```
auth: device flow start/poll, validate PAT, current user
repos: list mine, create (private, under user or org), resolve remote URL -> owner/repo, default branch, branch protection status
pulls: list open (filter: touches *.md), get, create, update, merge, mergeability
reviews: list comments (inline + general), create inline comment, approve, request changes
issues: find by label+path, create, list comments, comment
```

GitLab maps PR to MR, review comments to discussions, approve to approval API. Self-hosted GitLab is the same impl with a custom base URL.

## Provider polling

No webhooks on a desktop app. Poll on focus and every 60 seconds. GitHub: one GraphQL query for open PRs with review threads, REST with ETags elsewhere. GitLab: REST with `If-None-Match`. Back off to 5 minutes after 10 minutes unfocused.

## Auth apps

GitHub App and gitlab.com OAuth application registered under the `kmdn-app` org. Client IDs are public and compiled in, overridable with `KMDN_GITHUB_CLIENT_ID` and `KMDN_GITLAB_CLIENT_ID` at build time for forks and self-hosted setups. Custom GitLab endpoints use a personal access token.

## Distribution

GitHub Releases under `kmdn-app/kmdn`. Tauri updater with a signing key in CI. macOS: signed and notarized DMG. Linux: AppImage and .deb. Homebrew cask and Flatpak later.

## Diagnostics

No telemetry. Rolling local log file in the app data dir. "Copy diagnostics" button collects version, OS, provider kind, last 200 log lines, with paths and document content scrubbed.

## Codebase layout

```
kmdn/
  Cargo.toml              # workspace
  crates/
    kmdn-core/            # git, worktrees, providers, index, checks, agent adapters. No Tauri.
    kmdn-cli/             # kmdn-cli check | index | init. Used by CI and scripts.
  apps/
    desktop/              # Tauri 2 shell + React UI. Tauri commands call kmdn-core.
  packages/               # shared TS: types generated from Rust via ts-rs, UI kit
  templates/              # new-KB template, CI workflow files
  docs/
  spikes/                 # throwaway, deleted after each spike
```

pnpm workspace for `apps/` and `packages/`. One release pipeline builds the CLI and the desktop app.

## KB repo CI, optional

`kmdn-cli check` in a GitHub Action or GitLab CI job: broken relative links, invalid frontmatter, assets over the cap, stale `AGENTS.md`. Off by default. Settings offers "Add CI check to this knowledge base", which opens a PR with the workflow file.

## Process model

One Rust core per window. Agents are child processes spawned per thread session with the thread's worktree as cwd, each over its own stdio protocol behind a common adapter interface. Several agents run in parallel. Git operations run on a dedicated thread per repo; the UI never blocks.
