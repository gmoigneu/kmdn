# Spike 2: rendered block-aligned diff

Ran 2026-09-19. Result: **pass**. Own block alignment; no existing HTML diff tool needed.

## What was built

- `src/blocks.ts`: splits GFM into typed blocks with line ranges. Frontmatter, heading, paragraph, list item, fence (whole), table (whole), quote line, hr, html, blank.
- `src/align.ts`: LCS over normalized block text for equal pairs, then pairs the leftovers by kind in order as modified, the rest added or removed.
- `src/wordDiff.ts`: token LCS for prose, line LCS for fences, tables, frontmatter, html.
- `src/render.ts`: renders each block with `marked`, inserts private-use markers into the markdown source for changed tokens before rendering and swaps them for `<ins>`/`<del>` after, so highlights survive inline formatting. Each row carries `data-line-new` and `data-line-old`; `commentAnchor(row)` returns the PR side and line for a comment.

## Pass criteria

| Criterion | Result |
|---|---|
| 50-block document with insert, delete, edited paragraph | pass: 2 removed, 2 added, 1 modified, 17 ms |
| comment anchored to a rendered block maps to a line in the new file | pass: `commentAnchor` returns `{side: "RIGHT", line}` for blocks present in the new file, `LEFT` for removed blocks |
| word-level highlights in prose | pass: only `five` deleted and `ten`, `, then page on-call` inserted |
| line-level in code and tables | pass: fences and tables diff by line in a `<pre class="raw">` |

## Decisions

1. Own alignment. About 120 lines. `marked` is the only dependency and can be swapped for the same renderer the editor uses so all views agree (D17 says one parser everywhere).
2. Modified blocks render old and new side by side. Added and removed blocks render on one side only. Equal blocks render once, dimmed.
3. Tables and fences are one block each; a one-cell change highlights the row. Acceptable for v1.

## Known limits
- Moved blocks show as removed plus added, as planned in 07-editor.md.
- Nested lists split into one block per top-level item with its continuation lines.
- Marker injection assumes changed tokens never straddle a markdown delimiter in a way that breaks parsing. Rare in prose; if `marked` output ever contains a raw marker character, the fallback is line-level rendering for that block. Not implemented here.
