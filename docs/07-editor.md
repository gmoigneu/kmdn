# Editor

The editor is the Editor tab of a thread's right pane, and the Read tab when browsing. See [10-ui.md](10-ui.md) for placement.

## Mode

Source editor with live preview, Obsidian style. The document is always plain markdown text in a CodeMirror 6 buffer. Decorations render formatting in place: headings sized, emphasis styled, links clickable, images shown inline, code fences highlighted, tables aligned. Syntax markers appear on the active line and fade elsewhere.

Consequence: what is saved is exactly what was typed. No serializer, no round-trip risk, no diff noise.

## Markdown

GFM plus YAML frontmatter. Parser: a CommonMark plus GFM implementation with a frontmatter extension, shared between the editor decorations, the preview, the diff renderer, and the indexer so all four agree.

Not supported and rendered as plain text: wikilinks, math, Mermaid, admonitions, HTML blocks. HTML is preserved but shown as source.

## Frontmatter UI

Frontmatter block collapsed by default into a properties strip: title, status, owner, tags, reviewed. Editing the strip edits the YAML. Expanding shows raw YAML.

## Links

- Relative links to `*.md` open in kmdn. Renaming or moving a document offers to rewrite inbound links across the repo, inside the change set.
- Broken links underlined. Link picker with fuzzy search over titles and paths.

## Images

Paste or drop writes the asset and inserts `![](assets/<slug>/<hash>.png)`. Inline rendering in the editor.

## Read mode

Rendered view without editing affordances for browsing. Same renderer as the diff view.

## Rendered diff

Used in the review panel. Two renders of the document, old and new, aligned by block. Prose blocks diffed at word level. Code fences and tables at line level. Moved blocks are shown as removed plus added in v1.

## Search

Local full-text search over the SQLite FTS5 index. Filters: tag, status, owner, folder. Results show title, path, snippet.

## Accessibility and shortcuts

Keyboard first. Standard shortcuts for bold, italic, link, heading levels, save, command palette, quick open. Screen-reader labels on all panels.
