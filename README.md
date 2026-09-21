# kmdn

Collaborative markdown knowledge base for teams and the agents that work with them.

kmdn is a desktop app. A team keeps its knowledge base as plain markdown in a git repository. People edit by hand or by asking an agent. Every change goes through a review that is a normal pull request. Any agent can then read the knowledge base with a plain clone.

Website: [kmdn.dev](https://kmdn.dev).

Status: the golden path works end to end, and an end-to-end test drives it against a real GitHub repository. Core covers git, worktrees, sync, rebase with conflicts, checks, index, GitHub and GitLab providers, submit, and the agent adapters. Desktop has sign-in, clone or create a knowledge base, threads with a live-preview editor and an embedded agent (Claude Code, Codex, pi), rendered changes, the conflict resolver, discussions, the command palette, full-text search, notifications, submit for review, and the review layout with approve and publish. Not yet: packaged downloads, the updater, and a notarised macOS build. Read [docs/README.md](docs/README.md) for the full design, decisions, and spike plan, or [docs/user-guide.md](docs/user-guide.md) to use it.

## Shape

- Task-first UI modeled on the web Codex: a thread is a change set, a worktree, a branch, and eventually a PR.
- GitHub and GitLab, including self-hosted, behind one provider interface.
- Agents run through native adapters: Claude Code, Codex, pi. ACP as a fallback.
- Markdown only: GitHub Flavored Markdown plus YAML frontmatter.
- Tauri 2, React, Rust core with a CLI for CI.

## Layout

```
crates/kmdn-core   git, worktrees, providers, index, checks, agent adapters
crates/kmdn-cli    kmdn-cli check | index | init
apps/desktop       Tauri 2 shell and React UI
scripts/           gen-types.sh, the ts-rs export into the desktop app
templates/         new-KB template and CI workflow files
docs/              design
spikes/            throwaway experiments, deleted when done
```

## License

Apache-2.0. See [LICENSE](LICENSE).
