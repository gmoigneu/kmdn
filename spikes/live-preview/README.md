# Spike 1: live preview editor on CodeMirror 6

Ran 2026-09-19. Result: **pass**. Build decorations from scratch; no existing extension needed.

## What was built

`src/livePreview.ts`: one `ViewPlugin` that walks the Lezer markdown tree over the visible ranges and emits decorations.

- Syntax markers (`#`, `**`, `` ` ``, `[`, `](url)`, `>`, `~~`) are replaced with nothing on lines that do not intersect the selection, and shown as dimmed marks on active lines. List bullets stay visible.
- Headings, fences, quotes, tables get line classes. Emphasis, strong, strikethrough, inline code, links get inline classes.
- Images, task checkboxes, and horizontal rules become widgets when inactive.
- Frontmatter collapses to a one-line widget (`title · N properties`) unless the cursor is inside it, then shows as monospace lines.

`index.html` plus `src/main.ts` is a demo page with a 2,000-line generated document. Run `pnpm dev` in this folder.

## Pass criteria

| Criterion | Result |
|---|---|
| headings, emphasis, links, inline images, tables, task lists, fenced code rendered inline | pass, asserted in `test/livePreview.test.ts` |
| markers visible only on the active line | pass, asserted for header and bold markers |
| frontmatter collapsed | pass, collapsed when cursor outside, expanded inside |
| typing latency on a 2,000-line document | pass on CPU: 1.6 ms per rebuild over the whole document in jsdom. The real plugin only decorates visible ranges, so per-keystroke cost is lower. Paint cost not measured here, no browser on the build box. |

## Decisions

1. Build decorations ourselves. About 150 lines, all in one file, no dependency beyond `@codemirror/lang-markdown`. An external live-preview package would add a dependency for less control.
2. Fenced code highlighting comes from `@codemirror/lang-markdown` with `codeLanguages: languages`. Nothing to build.
3. The real editor adds: a `StateField` for the active-lines set so decorations only rebuild when the set changes, image widgets resolving relative paths against the worktree, and a click handler on links. All small.

## Not verified
- Paint performance and scroll smoothness in WebKitGTK and WKWebView. Check in the first desktop build.
- Setext headings and nested lists with mixed markers. The tree gives the nodes; the plugin covers `SetextHeading1/2` by name but no test exercises them.
