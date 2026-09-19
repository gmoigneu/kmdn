// Render aligned blocks as HTML: rendered markdown per block, changed words highlighted.
// Each row carries data-line-new so a comment on it maps to a PR line in the new file.
import { marked } from "marked";
import { Block, splitBlocks } from "./blocks";
import { align, Op } from "./align";
import { Seg, wordDiff, lineDiff } from "./wordDiff";

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]!));

function renderBlock(b: Block): string {
  if (b.kind === "frontmatter") return `<pre class="fm">${esc(b.text)}</pre>`;
  return marked.parse(b.text, { async: false, gfm: true }) as string;
}

// Highlight by inserting private-use markers into the markdown source, rendering, then swapping
// the markers for ins/del tags. Fences, tables, frontmatter and html use line-level raw diff.
const INS0 = String.fromCharCode(0xe000), INS1 = String.fromCharCode(0xe001);
const DEL0 = String.fromCharCode(0xe002), DEL1 = String.fromCharCode(0xe003);
function markSource(segs: Seg[], side: "a" | "b"): string {
  return segs.map((s) => {
    if (s.type === "equal") return s.text;
    if (s.type === "ins") return side === "b" ? INS0 + s.text + INS1 : "";
    return side === "a" ? DEL0 + s.text + DEL1 : "";
  }).join("");
}
function swapMarkers(html: string): string {
  return html.split(INS0).join("<ins>").split(INS1).join("</ins>").split(DEL0).join("<del>").split(DEL1).join("</del>");
}

function renderModified(op: { a: Block; b: Block }): { old: string; new: string } {
  const raw = op.a.kind === "fence" || op.a.kind === "table" || op.a.kind === "frontmatter" || op.a.kind === "html";
  if (raw) {
    const segs = lineDiff(op.a.text, op.b.text);
    const side = (s: "a" | "b") => `<pre class="raw">${segs.map((x) => {
      if (x.type === "equal") return esc(x.text);
      if (x.type === "ins") return s === "b" ? `<ins>${esc(x.text)}</ins>` : "";
      return s === "a" ? `<del>${esc(x.text)}</del>` : "";
    }).join("")}</pre>`;
    return { old: side("a"), new: side("b") };
  }
  const segs = wordDiff(op.a.text, op.b.text);
  return {
    old: swapMarkers(renderBlock({ ...op.a, text: markSource(segs, "a") })),
    new: swapMarkers(renderBlock({ ...op.b, text: markSource(segs, "b") })),
  };
}

export interface RenderedRow { type: Op["type"]; old?: string; new?: string; lineNew?: number; lineOld?: number; }

export function renderDiff(oldMd: string, newMd: string): RenderedRow[] {
  const ops = align(splitBlocks(oldMd), splitBlocks(newMd));
  return ops.map((op): RenderedRow => {
    switch (op.type) {
      case "equal": return { type: "equal", old: renderBlock(op.a), new: renderBlock(op.b), lineOld: op.a.startLine, lineNew: op.b.startLine };
      case "modified": { const r = renderModified(op); return { type: "modified", old: r.old, new: r.new, lineOld: op.a.startLine, lineNew: op.b.startLine }; }
      case "added": return { type: "added", new: `<ins class="block">${renderBlock(op.b)}</ins>`, lineNew: op.b.startLine };
      case "removed": return { type: "removed", old: `<del class="block">${renderBlock(op.a)}</del>`, lineOld: op.a.startLine };
    }
  });
}

/** First line to attach a PR comment to: new file when the block exists there, else old file. */
export function commentAnchor(row: RenderedRow): { side: "RIGHT" | "LEFT"; line: number } {
  return row.lineNew != null ? { side: "RIGHT", line: row.lineNew } : { side: "LEFT", line: row.lineOld! };
}

export function toHtml(rows: RenderedRow[]): string {
  return `<div class="rdiff">${rows.map((r) =>
    `<div class="row ${r.type}" data-line-new="${r.lineNew ?? ""}" data-line-old="${r.lineOld ?? ""}"><div class="old">${r.old ?? ""}</div><div class="new">${r.new ?? ""}</div></div>`
  ).join("")}</div>`;
}
