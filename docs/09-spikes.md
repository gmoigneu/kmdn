# Spikes and remaining items

The 15 open questions from the first draft were resolved on 2026-09-18. See D26 to D37 in [02-decisions.md](02-decisions.md). The UI round on 2026-09-19 added D38 to D54 and Spike 4.

## Spikes before product code

Throwaway code in `spikes/`, one week cap each, run in parallel. Each ends in a short write-up: pass or fail, what was learned, what changes in the plan.

### Spike 1: live preview editor (done 2026-09-19, pass, see spikes/live-preview/README.md)
Goal: CodeMirror 6 buffer rendering GFM in place.
Pass: headings, emphasis, links, inline images, tables, task lists, fenced code with highlighting, all rendered inline; syntax markers visible only on the active line; frontmatter collapsed; typing latency unnoticeable on a 2,000-line document.
Decide: build decorations from scratch versus start from an existing open-source live-preview extension.

### Spike 2: rendered diff (done 2026-09-19, pass, see spikes/rendered-diff/README.md)
Goal: two versions of a real document, rendered, aligned by block, word-level highlights in prose, line-level in code and tables.
Pass: a 50-block document with insertions, deletions, and an edited paragraph renders correctly; a comment can be anchored to a rendered block and mapped to a line number in the new file.
Decide: own block alignment versus adapting an existing HTML diff tool.

### Spike 3: native agent adapters (done 2026-09-19, pass for all three, see spikes/agents/README.md)
Goal: from Rust, drive each agent's official structured mode: `claude` stream-json, `codex app-server`, `pi --mode rpc`.
Pass per agent: session starts with the user's existing login, a reply streams, the agent attempts a write outside allowed paths and kmdn denies it, the file does not land on disk, an allowed write does, the session resumes after the process is restarted.
Decide: the shape of the internal interface after seeing three real wire formats, and whether pi's RPC exposes a usable permission event. Also record a fixture per agent for CI.

### Spike 4: worktrees with libgit2
Goal: create, list, remove worktrees from git2-rs; rebase a worktree branch onto origin/main; stash from the main clone and apply into a worktree.
Pass: three threads with worktrees, two agents writing concurrently, no cross-talk, rebase of one worktree while another is dirty. Half a week.

## Remaining items

1. **Domain**: kmdn.dev has no DNS records. Confirm with a registrar and buy before announcing.
2. **GitHub org**: create `kmdn-app`, register the GitHub App and gitlab.com application there.
3. **Apple Developer account**: needed for notarization before the first external macOS user.
4. **Test projects**: one GitHub repo and one GitLab project for the nightly end-to-end run.
5. **Watcher edge cases**: atomic rename saves from other editors, editor-in-place saves, symlinked clones. Handle during the repo service build with tests.
6. **Templates**: new-KB template and the optional CI workflow files for GitHub and GitLab.
7. **Frontmatter write behavior**: confirm a YAML library that preserves key order, comments, and quoting on partial edits. Candidate check during the indexer build.
