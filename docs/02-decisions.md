# Decisions

Format: decision, options considered, why. Dated 2026-09-18. Change a decision by adding a new entry that supersedes it, not by editing.

## D1. First user: mixed dev and non-dev team
Options: mixed team, devs only, solo agent-heavy, external customers.
Why: forces git to be hidden without dropping it. Devs-only would let git leak into the UI and never get fixed.

## D2. V1 done means: non-dev edits, dev reviews, merged, external agent reads
Options: that, agent-first rewrite flow, collaboration-first, minimal editor.
Why: exercises storage, review, and consumption. Everything else is a feature on top.

## D3. One KB = one repo, one open at a time
Options: one repo, folder inside any repo, multi-repo aggregate.
Why: simplest model. Multi-repo becomes a workspace switcher later.

## D4. Providers: GitHub and GitLab, including self-hosted GitLab, behind an abstraction from day one
Options: GitHub only, both, plain git only, plain git plus optional provider.
Why: the team needs both. Abstraction is cheap now and expensive later.

## D5. A review is a provider PR or MR
Options: provider PR, kmdn-native review stored in git, hybrid.
Why: approvals, inline comments, notifications, permissions, and CI for free. Devs can review from the provider UI too. Hybrid means two sources of truth.

## D6. Edits are grouped into change sets, one branch per change set
Options: change set, branch per doc edit, long-lived branch per user, direct-to-main for trusted users.
Why: maps one to one onto a PR without saying branch. Cross-doc edits stay in one review.

## D7. Full local clone at a user-chosen path (amended by D40)
Options: app-managed clone, user-chosen path, API only.
Why: devs use the same clone with their tools. Offline works. Agents run on real files. Cost: external edits, handled by D8.

## D8. kmdn watches the working tree and adopts external changes into the active change set (superseded by D45)
Options: adopt, refuse to act on a dirty tree, ignore.
Why: devs will edit the clone with other tools. Refusing blocks them, ignoring loses work. kmdn never rewrites history it did not create.

## D9. Git via bundled libgit2 (git2-rs), HTTPS with the provider token
Options: libgit2, shell to system git, libgit2 with system git fallback, gitoxide.
Why: no system dependency for non-devs. One code path. SSH remotes are rewritten to HTTPS for kmdn's own network operations; the user's own git config is untouched.

## D10. Auth: OAuth device flow for github.com and gitlab.com, personal access token for custom endpoints
Options: that, PAT everywhere, full OAuth including self-hosted app registration.
Considered and rejected: a relay server or shared bot token so non-devs need no provider account. Rejected for v1 because it means running a server or sharing a credential. Provider accounts are required. The provider abstraction leaves room for a relay backend later.
Tokens live in the OS keychain.

## D11. Document metadata in YAML frontmatter
Options: frontmatter, sidecar files, central index file, SQLite in repo.
Why: travels with the file, visible to every tool and agent, reviewed in the same PR. Cross-doc queries come from a local index, not from the repo.

## D12. Local app state in an app-managed SQLite, never committed
Search index, provider cache, UI state. Rebuildable from the repo at any time.

## D13. Comments outside reviews are provider issues, one per document, linked by path
Options: issues, none in v1, inline HTML comments, kmdn-native.
Why: threads, mentions, notifications, identity for free. Visible in the provider UI.

## D14. Agents speak Agent Client Protocol (ACP) and run as local subprocesses (superseded by D55)
Options: ACP, per-CLI headless JSON adapters, vendor SDKs with kmdn-held keys, Claude Code only.
Why: one client for pi, Claude Code, and Codex. Users install and log into agent CLIs themselves, so billing and identity are theirs.

## D15. Agents edit the working tree inside the active change set, markdown and assets only, user reviews the diff before commit
Options: that, separate worktree, full access with per-call approval, propose-only.
Why: agents keep their tools, users keep control. Writes outside allowed paths are denied through ACP permission handling.

## D16. Editor: source with live preview, Obsidian style
Options: WYSIWYG with markdown toggle, source with live preview, split pane, block editor with JSON model.
Why: markdown stays visible and round-trip is exact by construction. No serializer to get wrong.

## D17. Markdown flavor: GitHub Flavored Markdown plus YAML frontmatter, nothing else
Options: GFM only, plus wikilinks, plus Mermaid/math/admonitions, MDX or Markdoc.
Why: renders identically on GitHub and GitLab, so out-of-app review looks right. Every agent understands it. Relative links only.

## D18. Attachments: images only, stored next to the doc, size cap, committed in the change set
Options: images only, any file, external storage, none.
Why: screenshots are unavoidable. Everything else bloats the repo or breaks offline.

## D19. Navigation mirrors the folder tree, title from frontmatter, optional order key
Options: folder tree, curated manifest, flat tag-based.
Why: what you see is what is on disk. Agents walk the same tree.

## D20. External agents consume a plain clone plus a generated index file (amended by D56)
Options: clone plus index, clone only, kmdn MCP server, hosted API.
Why: zero runtime. The index gives agents an entry point and a map. Regenerated inside each change set.

## D21. Change sets auto-rebase on main; conflicts resolved in a guided block-level resolver
Options: rebase with guided resolver, merge and defer to devs, pessimistic locks, do nothing.
Why: non-devs never see conflict markers. Rebase implies force-push of the change-set branch, acceptable because each branch has one owner.

## D22. Review UX: all open PRs touching markdown appear; reviewer reads rendered diff, comments inline, approves, requests changes, merges
Options: all markdown PRs, kmdn-created only, list and link out.
Why: PRs opened from a terminal must be reviewable by non-devs. Merge calls the provider and inherits its permission checks.

## D23. Platforms: macOS and Linux in v1, Windows later
Why: agent CLIs and subprocess handling are most reliable there. Windows after ACP launchers are proven.

## D24. Open source, Apache-2.0
Why: permissive, company friendly. OAuth client IDs for device flow are public by design.

## D25. Explicitly not in v1
Live multi-user editing, Windows build, MCP server and semantic search, relay server and kmdn-native identity, telemetry. See [08-not-in-v1.md](08-not-in-v1.md).

## D26. Change set title and PR body are generated by the embedded agent at submit
Options: auto from first document, ask at start, agent-generated.
Why: good titles without asking non-devs to write them. Branch slug still comes from the first document touched, since the branch exists before submit. Fallback when no agent is installed: "Update <doc title>" or "Update N documents", body is the list of changed docs. Existing repo PR templates are appended.

## D27. Commit on explicit save only
Options: save plus idle debounce, save only, single amended commit, local drafts.
Why: predictable history, no surprise commits. Unsaved buffer text is mirrored to local SQLite for crash recovery, never to git.

## D28. Three throwaway spikes before product code
Live preview, rendered block-aligned diff, ACP with three agents. One week cap each, in parallel, in `spikes/`, each with a pass/fail criterion. See [09-spikes.md](09-spikes.md).

## D29. One review model across providers, degrade to structured comments
Options: one model with degradation, show provider differences, GitLab approvals only.
Why: non-devs see the same buttons everywhere. Native approval APIs used when present. Otherwise kmdn posts a marker comment such as `<!-- kmdn:approved -->` plus human text and parses markers back. Merge gating stays with the provider.

## D30. GitHub App and gitlab.com application under the kmdn-app org, public client IDs, overridable at build time
Options: GitHub App, classic OAuth App, personal account.
Why: fine-grained permissions, org admins see what kmdn accesses, better rate limits. Env var lets forks and self-hosted deployments use their own IDs.

## D31. Freshness by polling: on focus and every 60s, conditional requests, GraphQL batching on GitHub, back off when idle
Options: that, manual refresh only, 15s polling.
Why: a desktop app cannot receive webhooks. ETags make unchanged polls free. Unfocused for 10 minutes drops to every 5 minutes.

## D32. kmdn follows HEAD when the terminal moves it (superseded by D45)
Options: follow, warn and re-checkout, treat any branch as a change set.
Why: kmdn never fights the user. Known `kmdn/` branch: switch to that change set. Default branch: no change set. Unknown branch: read-only with an "adopt as change set" action. Rebase or merge in progress: banner, writes disabled.

## D33. Scale target: 5,000 documents, 1 GB clone
Tree renders from the filesystem immediately, FTS index fills in the background, incremental reindex on file events.

## D34. Distribution: GitHub Releases, Tauri updater, signed and notarized DMG, AppImage and .deb
Homebrew cask and Flatpak when asked. Needs an Apple Developer account and a Tauri updater key in CI secrets.

## D35. No telemetry in v1
Local log file plus a "copy diagnostics" button. Only network calls are to the user's provider, the agents' own vendors, and the updater.

## D36. Testing: heavy on the Rust core, component tests on editor and diff, nightly end-to-end
Rust integration tests against local bare repos and recorded provider responses. UI component tests for editor decorations and rendered diff. Nightly e2e against a real GitHub and GitLab test project, not on PR CI.

## D37. Home: GitHub org kmdn-app, product name kmdn, domain kmdn.dev
GitHub handle `kmdn` is taken by an active account. npm, crates.io, PyPI, Homebrew names are free as of 2026-09-18. Domain availability checked by DNS only, confirm with the registrar before announcing.

## UI round, 2026-09-19

Reference is the web Codex at chatgpt.com/codex.

## D38. Task-first interaction model
Options: Codex layout and model, layout with editor in center, visual style only, parallel threads only.
Why: the primary gesture is "ask for a change", by hand or by agent. The editor becomes a pane, not the app.

## D39. Thread = change set. Conversation optional
Options: that, thread = agent conversation, thread = document.
Why: one concept. Manual edits, agent turns, submit, and review comments share a timeline.

## D40. Worktree per thread, main clone stays on the default branch
Options: worktrees, single checkout, worktrees for agent threads only.
Why: parallel agents, no branch switching, the user's clone stays clean. Worktrees live in a sibling folder `<clone>.kmdn-worktrees/<slug>`. Amends D7: the user-chosen path is the main clone, edits happen in worktrees.

## D41. Home is a composer plus task list
Options: composer with sidebar, composer only, documents tree with floating composer.
Why: matches web Codex. Starting a task is one keystroke, the sidebar keeps browsing and reviewing one click away.

## D42. Sidebar: KB switcher, search, Threads, Reviews, Documents
Options: that, threads and reviews only, icon rail with panels.
Why: non-devs need to browse. One column, collapsible sections.

## D43. Thread view is two columns; sidebar collapses to an icon strip
Options: two columns, three always, sidebar hidden.
Why: diff and editor need width. Home and Review keep the full sidebar.

## D44. Right pane tabs: Changes, Editor, Read
Options: tabs, diff inline in the conversation, split pane.
Why: long documents need a full pane for diff and for editing. Changes is the default once anything changed.

## D45. Main clone dirty state appears as a read-only Local changes thread
Options: that, ignore, adopt into the last used thread.
Why: devs keep their habits, kmdn shows what is there and offers Move to new thread or Adopt branch. kmdn never commits in the main clone. Supersedes D8 and D32.

## D46. Composer controls: agent, mode (Suggest, Edit, Developer), @doc, image; model only if the agent exposes it
Options: that, agent and text only, full parity with reasoning effort.
Why: Suggest and Edit map to Codex Ask and Code. Developer hides shell access behind a setting.

## D47. Editing from Read: one click, thread created if none selected
Options: one click with auto thread, always ask, edit directly in main clone.
Why: no modal before a typo fix. The new thread appearing in the sidebar teaches the model.

## D48. Dedicated review layout: full-width diff, comments drawer
Options: same layout as a thread, dedicated layout, reviews as editable threads.
Why: reviewers read more than they write. Second layout accepted as cost.

## D49. Thread header: status pill plus one primary action that follows state
Options: header bar, composer actions, sidebar context menu only.
Why: one button always does the next right thing. Merged threads go to Done and are cleaned up after 7 days.

## D50. Visual: Codex density and chrome, reading-optimized panes, light and dark, one accent
Options: that, dark-only clone, Notion-like.
Why: dense where you scan, comfortable where you read.

## D51. UI stack: Tailwind, shadcn/ui on Radix, customized tokens
Options: that, hand-rolled CSS modules, full kit.
Why: own the look, keep accessible primitives.

## D52. Cmd-K palette for navigation and every action; every action also reachable by mouse
Options: everything, navigation only, none.
Why: devs get speed, non-devs get discoverability.

## D53. First run: sign in, pick repo, choose folder; agents detected afterwards
Options: that, open folder first, require an agent.
Why: browsing, editing, and reviewing work with no agent installed.

## D54. OS notifications for three events only: agent needs approval, agent finished while unfocused, review requested
Options: three events, badges only, everything.
Why: rare interruptions stay enabled. The rest is badges and dots.

## D55. Agents run through native adapters behind one internal interface; ACP is a fallback adapter
Options: native adapters, keep ACP as core, PTY terminals like Orca, Claude only.
Why: no shipping wrapper app uses ACP as its core. Claude Code and pi would both depend on community bridges with known permission gaps, and Anthropic's rules on subscription use through the SDK and ACP are in flux. Each vendor's own structured protocol is the best maintained path and lets users run their own logged-in CLIs. Research in [11-agent-integration-research.md](11-agent-integration-research.md). Supersedes D14. Decided 2026-09-19.
- Claude Code: user's `claude` CLI, stream-json in and out, `--permission-prompt-tool`, `--disallowedTools` path rules.
- Codex: user's `codex app-server` over stdio, JSON-RPC 2.0.
- pi: user's `pi --mode rpc`.
- Others later: generic ACP adapter.

## Pre-implementation round, 2026-09-19

## D56. AGENTS.md is generated inside threads and treated as derived: on rebase conflict kmdn discards both sides and regenerates
Options: CI job on main, regenerate on conflict, drop the file, client commits to main after publish.
Why: works with no CI. Flaw fixed: two parallel threads no longer block each other on the index file when kmdn does the rebase. Known gap: terminal merges of two doc PRs still conflict on it, and `kmdn-cli check` reports a stale index so the fix is one command. Amends D20.

## D57. Publish defers entirely to the provider
Options: kmdn enforces approvals from config, defer to provider, block self-merge only.
Why: one source of truth for merge rules, consistent with terminal merges. The advisory `require_approvals` config field is removed. kmdn shows a one-time hint when the default branch has no protection.

## D58. Condensed agent log posted to the PR as one collapsible comment; full transcript stays local
Options: condensed comment, local only, full transcript committed.
Why: reviewers see intent, the PR keeps a record, nothing sensitive leaves the machine unless the author lets it. Author can edit or remove the log before submit. Updated on each push.

## D59. Monorepo: crates/kmdn-core, crates/kmdn-cli, apps/desktop
Options: monorepo with core library, single Tauri app, separate repos.
Why: core has no Tauri dependency and is tested alone. CLI wraps core for CI and scripting. Desktop wraps core in Tauri commands. Cargo workspace plus pnpm workspace, one release pipeline.

## D60. Optional CI check for KB repos via kmdn-cli
Options: yes opt-in, local only, not in v1.
Why: keeps terminal-made changes honest without forcing CI on anyone. Checks: broken relative links, invalid frontmatter, oversize assets, stale AGENTS.md. kmdn offers to add the workflow file once, from settings, never automatically. Same checks run locally before submit.

## D61. New knowledge base from inside kmdn, created on the provider from a template, private by default
Options: yes in v1, existing repos only, initialize an empty repo.
Why: zero to KB in one flow for a non-dev. Adds create-repo to the provider trait and a small template: config, README, one example doc, AGENTS.md.
