# Content model

## Repo layout

```
repo/
  .kmdn/
    config.yaml          # KB config, committed
  AGENTS.md              # generated index for agents, committed
  <any folders>/
    doc.md
    assets/<doc-slug>/<hash>.png
```

Rules:
- Any `*.md` outside `.kmdn/` is a document. Folders are navigation.
- `README.md` in a folder is that folder's landing page.
- Non-markdown, non-image files are shown greyed out and never edited by kmdn or agents.
- kmdn works on a repo with no `.kmdn/config.yaml`. Defaults apply.

## Frontmatter schema

All fields optional. Unknown fields are preserved untouched.

```yaml
---
title: Deploy runbook          # shown in tree, defaults to first H1, then filename
description: One-line summary  # used in AGENTS.md
owner: "@alice"                # provider handle
tags: [ops, deploy]
status: draft | review | published | deprecated
order: 10                      # sibling sort, ascending, untitled last
reviewed: 2026-09-01           # last human validation date
---
```

kmdn writes only fields the user or an agent changed. It never reorders or reformats existing frontmatter.

## AGENTS.md, generated

Regenerated on submit inside the thread. Treated as derived: when a rebase conflicts on this file, kmdn discards both sides and regenerates from the rebased tree. `kmdn-cli check` flags a stale index. Contents:
- One paragraph on what the KB is, from `.kmdn/config.yaml` `description`.
- How to read it: GFM, frontmatter fields, relative links.
- Tree of documents with path, title, description, status. Deprecated docs marked.

Never edited by hand. A header line says so. Terminal merges that conflict on it: run `kmdn-cli index` and commit.

## Assets

- Paste or drop an image: written to `assets/<doc-slug>/<sha256-prefix>.<ext>` next to the document, relative link inserted.
- Formats: png, jpg, gif, webp, svg.
- Default cap 5 MB per file, warning at 1 MB. Configurable.
- Orphan detection is a later feature.

## `.kmdn/config.yaml`

```yaml
version: 1
name: Platform KB
description: Internal knowledge base for the platform team.
default_branch: main            # detected if absent
branch_prefix: kmdn/            # change set branches
review:
  labels: [kmdn]                # applied to PRs and issues
  post_agent_log: true          # condensed agent log as a PR comment
assets:
  max_bytes: 5242880
agents:
  allowed_paths: ["**/*.md", "**/assets/**"]
```

## New KB template

Created by "New knowledge base": private repo on the provider, first commit with `.kmdn/config.yaml`, `README.md`, `getting-started.md` as an example document, `AGENTS.md`, and optionally `.github/workflows/kmdn-check.yml` or `.gitlab-ci.yml` if the user opts in.

## Local state, not committed

App data dir, per repo, keyed by remote URL: SQLite with FTS5 over document bodies and frontmatter, provider caches, last-opened, panel layout. Deleting it is always safe.
