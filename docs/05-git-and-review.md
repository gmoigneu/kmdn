# Git and review

## Vocabulary

| UI word | Git or provider thing |
|---|---|
| Knowledge base | repo |
| Document | `*.md` file |
| Thread | change set = worktree plus branch `kmdn/<user>/<slug>` plus its commits and timeline |
| Submit for review | push branch, open PR/MR |
| Review | PR/MR |
| Publish | merge to default branch |
| Discussion | issue labeled `kmdn`, titled with the doc path |

Git words never appear in the default UI. A "details" view can show branch and commit ids for devs.

## Worktree model

The user-chosen path is the main clone. kmdn keeps it on the default branch and never commits there. Every thread gets a worktree at `<clone>.kmdn-worktrees/<slug>` on branch `kmdn/<user>/<slug>`. Agents run with the worktree as their working directory. Several threads run in parallel. Merged or abandoned threads have their worktree removed, merged ones after 7 days.

## Thread lifecycle

1. **Start**: from the Home composer, from Edit on a document, or from Move to new thread on Local changes. Creates the branch from origin/main and a worktree.
2. **Edit**: explicit save is a commit. No idle auto-commit. Message auto-generated, e.g. `Update deploy runbook`. Unsaved buffer text is mirrored to local SQLite for crash recovery. The provider squashes on merge if configured.
3. **Adopt external edits**: watcher sees modified or new `*.md` or asset files. If a change set is active, they are staged and committed on the next save. If on main and the tree is dirty, the UI asks to start a change set or leave the files alone. Files outside allowed paths are shown but never committed by kmdn.
4. **Submit**: run local checks (links, frontmatter, asset size), regenerate `AGENTS.md`, commit, push. The embedded agent proposes a title and a three-line summary from the diff; the user edits or accepts. Body: summary, list of changed documents with links, then the repo's PR template if one exists. Fallback without an agent: `Update <doc title>` or `Update N documents`, body is the list. Labels from config. If the thread had agent turns, a condensed log is posted as one collapsible PR comment: user prompts, one line per agent turn, files touched. The author previews and can edit or drop it. Updated on each push.
5. **Iterate**: further saves commit and push to the same branch. PR updates automatically.
6. **Publish**: merge from the review panel. kmdn deletes the remote branch if the provider did not, checks out main, pulls, deletes the local branch.
7. **Abandon**: closes the PR if any, deletes branches. Confirmed.

Several threads exist at once. Switching threads switches panes, not branches.

## The main clone and external tools

Devs keep using the main clone with their editor and terminal. kmdn watches it and shows an implicit read-only **Local changes** thread when the working tree is dirty or HEAD is off the default branch.
- Move to new thread: stash, create a thread, apply into its worktree, drop the stash on success.
- Adopt branch: register a non-default branch as a thread by creating a worktree for it. The branch is not renamed.
- A rebase, merge, or cherry-pick in progress in any worktree shows a banner and disables writes there.
- Edits made by external tools inside a kmdn worktree are committed on the next save in that thread.

## Sync

- Fetch every 60 seconds and on window focus.
- Main clone, clean: fast-forward the default branch.
- Each thread worktree: rebase onto origin/main. Clean rebase is silent and force-pushed if the branch was already pushed. `--force-with-lease` semantics via libgit2 refspec checks.
- Rebase touching a conflict: stop, open the resolver. Exception: a conflict only on `AGENTS.md` is resolved silently by regenerating it.

## Conflict resolver

Block level, not line level. A block is a paragraph, list item, heading, table row, or code fence.
- Left: my version. Right: their version on main. Per block: keep mine, keep theirs, keep both, edit.
- Frontmatter conflicts are resolved per key.
- Asset conflicts: keep both, rename mine.
- No conflict markers are ever written to disk in a state the user can see. If the resolver is cancelled, the rebase is aborted and the change set stays as it was.

## Review panel

Lists open PRs on the repo where the diff touches `*.md` or assets, regardless of who or what opened them.

Per review:
- Documents changed, each with a rendered diff: markdown rendered, changes highlighted at word level for prose and line level for code fences and tables.
- Raw diff toggle.
- Inline comment on a rendered block maps to a line comment on the PR at the block's first changed line.
- Existing PR comments from the provider are shown at their lines. Replies work.
- Actions: comment, approve, request changes, merge. Availability follows the provider's response, for example merge disabled when checks fail or approvals are missing. kmdn adds no rules of its own. If the default branch has no protection, kmdn shows a one-time hint pointing to the provider's settings.
- Same actions on every provider. Native approval and request-changes APIs are used when available. When the provider or tier lacks them, kmdn posts a comment with a hidden marker such as `<!-- kmdn:approved -->` and parses markers back, so state is consistent across kmdn clients. Merge gating always stays with the provider.
- Freshness: poll on window focus and every 60 seconds with conditional requests, GraphQL batching on GitHub, dropping to every 5 minutes after 10 minutes unfocused. Manual refresh always available.
- Merge strategy from provider defaults.

## Discussions on published docs

- "Discuss" on a document finds an open issue labeled `kmdn` whose title equals the doc path, or creates one.
- Thread shown in the comments panel. New comments post to the issue.
- Renaming a doc updates the issue title on next submit.
- Closing the issue is done from kmdn or the provider.

## Identity

Commits: author and committer are the logged-in provider user, name and email from the provider profile. Email may be the provider noreply address if the user hides theirs.

## What kmdn never does

- Rewrite history on main or on branches not prefixed `kmdn/`.
- Touch files outside allowed paths.
- Run user git hooks. libgit2 does not execute hooks. Document this for devs.
- Sign commits in v1.
