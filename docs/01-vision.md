# Vision

## Problem

Teams want a knowledge base that agents can trust. Wikis and Notion are hard for agents to read and impossible to review. Git repos of markdown are perfect for agents but hostile to non-developers.

kmdn puts a friendly editor and review flow on top of a git repo of markdown. Humans and agents maintain it together. Any agent can consume it with a plain clone.

## First users

A mixed team: some developers, some not. Everyone has a GitHub or GitLab account. Developers keep using their terminal and editor on the same repo.

## Golden path for v1

1. A non-developer opens the KB in kmdn, starts a thread from the composer or from a document, edits by hand or with an agent, submits it for review.
2. A developer sees the review in kmdn, reads the rendered diff, comments, approves, merges.
3. The change lands on main.
4. Claude Code in a terminal pulls the repo and reads the updated document.

If this works end to end, v1 is done.

## Principles

- Git is plumbing. The UI speaks documents, changes, and reviews.
- The repo is the source of truth. Nothing kmdn needs lives only in kmdn.
- Everything must survive a plain clone with no kmdn present.
- Reuse the provider. PRs, comments, approvals, notifications, permissions come from GitHub or GitLab.
- Agents are users. They edit inside the same threads and go through the same review.
- Task-first. Starting a change is the primary gesture. The editor is a pane, not the app. Reference: the web Codex.
