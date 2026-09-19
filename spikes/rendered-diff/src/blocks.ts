// Split GFM markdown into blocks with line ranges. Blocks: heading, paragraph, list item,
// fence (whole), table row, blockquote line, hr, blank, html. Frontmatter is one block.

export type BlockKind = "frontmatter" | "heading" | "paragraph" | "list_item" | "fence" | "table" | "quote" | "hr" | "blank" | "html";
export interface Block { kind: BlockKind; text: string; startLine: number; endLine: number; }

const LIST = /^\s*([-*+]|\d+[.)])\s/;
const BLOCK_START = /^(#{1,6}\s|```|~~~|\||>|\s*([-*+]|\d+[.)])\s)/;

export function splitBlocks(md: string): Block[] {
  const lines = md.split("\n");
  const out: Block[] = [];
  let i = 0;
  const push = (kind: BlockKind, s: number, e: number) => out.push({ kind, text: lines.slice(s, e + 1).join("\n"), startLine: s + 1, endLine: e + 1 });

  if (lines[0] === "---") {
    let j = 1;
    while (j < lines.length && lines[j] !== "---") j++;
    if (j < lines.length) { push("frontmatter", 0, j); i = j + 1; }
  }
  while (i < lines.length) {
    const l = lines[i];
    if (/^\s*$/.test(l)) { push("blank", i, i); i++; continue; }
    if (/^(```|~~~)/.test(l)) {
      const fence = l.match(/^(`{3,}|~{3,})/)![1];
      let j = i + 1;
      while (j < lines.length && !lines[j].startsWith(fence)) j++;
      push("fence", i, Math.min(j, lines.length - 1)); i = j + 1; continue;
    }
    if (/^#{1,6}\s/.test(l)) { push("heading", i, i); i++; continue; }
    if (/^(-{3,}|\*{3,}|_{3,})\s*$/.test(l)) { push("hr", i, i); i++; continue; }
    if (/^\|/.test(l)) {
      let j = i;
      while (j + 1 < lines.length && /^\|/.test(lines[j + 1])) j++;
      push("table", i, j); i = j + 1; continue;
    }
    if (/^>/.test(l)) { push("quote", i, i); i++; continue; }
    if (/^<[a-zA-Z!]/.test(l)) { push("html", i, i); i++; continue; }
    if (LIST.test(l)) {
      let j = i;
      while (j + 1 < lines.length && /^\s{2,}\S/.test(lines[j + 1]) && !LIST.test(lines[j + 1])) j++;
      push("list_item", i, j); i = j + 1; continue;
    }
    let j = i;
    while (j + 1 < lines.length && !/^\s*$/.test(lines[j + 1]) && !BLOCK_START.test(lines[j + 1])) j++;
    push("paragraph", i, j); i = j + 1;
  }
  return out;
}
