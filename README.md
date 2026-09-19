# kmdn

Collaborative markdown knowledge base for teams and the agents that work with them.

kmdn is a desktop app. A team keeps its knowledge base as plain markdown in a git repository. People edit by hand or by asking an agent. Every change goes through a review that is a normal pull request. Any agent can then read the knowledge base with a plain clone.

Status: the golden path works end to end and is verified nightly against a real GitHub repository. Core covers git, worktrees, sync, rebase with conflicts, checks, index, GitHub and GitLab providers, submit, agent parsers. Desktop has sign-in, clone or create a KB, threads with a live-preview editor and an embedded agent (Claude Code, Codex, pi), rendered changes, submit for review, and the review layout with approve and publish. Not yet: conflict resolver UI, discussions, command palette, notifications, packaging. Read [docs/README.md](docs/README.md) for the full design, decisions, and spike plan.

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
packages/          shared TypeScript
templates/         new-KB template and CI workflow files
docs/              design
spikes/            throwaway experiments, deleted when done
```

## License

Apache-2.0. See [LICENSE](LICENSE).
