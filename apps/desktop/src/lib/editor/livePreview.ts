// Spike 1: Obsidian-style live preview for CodeMirror 6.
// Approach: one ViewPlugin walks the Lezer markdown tree for the visible ranges and builds
// decorations. Syntax markers are hidden (Decoration.replace) except on the lines that
// intersect the selection. Headings, emphasis, code, links, quotes, tables get classes.
// Images become widgets. Frontmatter collapses to a one-line widget unless the cursor is inside.

import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate, WidgetType } from "@codemirror/view";
import { RangeSetBuilder, Extension } from "@codemirror/state";
import { syntaxTree } from "@codemirror/language";
import { SyntaxNodeRef } from "@lezer/common";

class ImageWidget extends WidgetType {
  constructor(readonly src: string, readonly alt: string) { super(); }
  eq(o: ImageWidget) { return o.src === this.src && o.alt === this.alt; }
  toDOM() { const img = document.createElement("img"); img.src = this.src; img.alt = this.alt; img.className = "cm-lp-img"; return img; }
  ignoreEvent() { return false; }
}
class HrWidget extends WidgetType { toDOM() { const s = document.createElement("span"); s.className = "cm-lp-hr"; return s; } }
class TaskWidget extends WidgetType {
  constructor(readonly checked: boolean) { super(); }
  eq(o: TaskWidget) { return o.checked === this.checked; }
  toDOM() { const b = document.createElement("input"); b.type = "checkbox"; b.checked = this.checked; b.className = "cm-lp-task"; return b; }
}
class FrontmatterWidget extends WidgetType {
  constructor(readonly summary: string) { super(); }
  eq(o: FrontmatterWidget) { return o.summary === this.summary; }
  toDOM() { const s = document.createElement("span"); s.className = "cm-lp-fm-collapsed"; s.textContent = this.summary; return s; }
}

const mark = Decoration.mark({ class: "cm-lp-mark" });
const hidden = Decoration.replace({});
const lineCls = (c: string) => Decoration.line({ class: c });
const inline = (c: string) => Decoration.mark({ class: c });

const HEADING: Record<string, string> = {
  ATXHeading1: "cm-lp-h1", ATXHeading2: "cm-lp-h2", ATXHeading3: "cm-lp-h3",
  ATXHeading4: "cm-lp-h3", ATXHeading5: "cm-lp-h3", ATXHeading6: "cm-lp-h3",
  SetextHeading1: "cm-lp-h1", SetextHeading2: "cm-lp-h2",
};
const MARKS = new Set(["HeaderMark", "EmphasisMark", "CodeMark", "LinkMark", "QuoteMark", "StrikethroughMark", "ListMark"]);

/** Frontmatter range if the doc starts with a `---` fence. Returns [from, to] covering the block. */
export function frontmatterRange(doc: string): [number, number] | null {
  if (!doc.startsWith("---\n")) return null;
  const end = doc.indexOf("\n---", 3);
  if (end < 0) return null;
  const lineEnd = doc.indexOf("\n", end + 1);
  return [0, lineEnd < 0 ? doc.length : lineEnd];
}

function activeLines(view: EditorView): Set<number> {
  const s = new Set<number>();
  for (const r of view.state.selection.ranges) {
    const a = view.state.doc.lineAt(r.from).number, b = view.state.doc.lineAt(r.to).number;
    for (let i = a; i <= b; i++) s.add(i);
  }
  return s;
}

export function buildDecorations(view: EditorView): DecorationSet {
  const { state } = view;
  const active = activeLines(view);
  const isActive = (from: number, to: number) => {
    const a = state.doc.lineAt(from).number, b = state.doc.lineAt(to).number;
    for (let i = a; i <= b; i++) if (active.has(i)) return true;
    return false;
  };
  const ranges: { from: number; to: number; deco: Decoration }[] = [];
  const push = (from: number, to: number, deco: Decoration) => ranges.push({ from, to, deco });

  // Frontmatter: collapse unless cursor inside.
  const fm = frontmatterRange(state.doc.toString());
  const fmActive = fm ? isActive(fm[0], fm[1]) : false;
  if (fm && !fmActive) {
    const body = state.doc.sliceString(4, Math.max(4, fm[1] - 4));
    const title = /^title:\s*(.+)$/m.exec(body)?.[1]?.trim();
    const keys = body.split("\n").map((l) => l.split(":")[0].trim()).filter((k) => k && !k.startsWith("#"));
    push(fm[0], fm[1], Decoration.replace({ widget: new FrontmatterWidget(title ? `${title} · ${keys.length} properties` : `${keys.length} properties`), block: true }));
  } else if (fm) {
    for (let l = state.doc.lineAt(fm[0]).number; l <= state.doc.lineAt(fm[1]).number; l++) {
      const line = state.doc.line(l); push(line.from, line.from, lineCls("cm-lp-fm"));
    }
  }

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from, to,
      enter: (n: SyntaxNodeRef) => {
        if (fm && n.to <= fm[1] && n.from >= fm[0]) return false;
        const name = n.name;
        const act = isActive(n.from, n.to);
        if (HEADING[name]) {
          for (let l = state.doc.lineAt(n.from).number; l <= state.doc.lineAt(n.to).number; l++) push(state.doc.line(l).from, state.doc.line(l).from, lineCls(HEADING[name]));
          return;
        }
        if (name === "FencedCode") {
          for (let l = state.doc.lineAt(n.from).number; l <= state.doc.lineAt(n.to).number; l++) push(state.doc.line(l).from, state.doc.line(l).from, lineCls("cm-lp-fence"));
          return; // children (CodeMark, CodeInfo) handled below via iteration continuing
        }
        if (name === "Blockquote") { for (let l = state.doc.lineAt(n.from).number; l <= state.doc.lineAt(n.to).number; l++) push(state.doc.line(l).from, state.doc.line(l).from, lineCls("cm-lp-quote")); return; }
        if (name === "Table") { for (let l = state.doc.lineAt(n.from).number; l <= state.doc.lineAt(n.to).number; l++) push(state.doc.line(l).from, state.doc.line(l).from, lineCls("cm-lp-table")); return; }
        if (name === "Emphasis") { push(n.from, n.to, inline("cm-lp-em")); return; }
        if (name === "StrongEmphasis") { push(n.from, n.to, inline("cm-lp-strong")); return; }
        if (name === "Strikethrough") { push(n.from, n.to, inline("cm-lp-strike")); return; }
        if (name === "InlineCode") { push(n.from, n.to, inline("cm-lp-code")); return; }
        if (name === "HorizontalRule") { if (!act) push(n.from, n.to, Decoration.replace({ widget: new HrWidget() })); return false; }
        if (name === "TaskMarker") { if (!act) push(n.from, n.to, Decoration.replace({ widget: new TaskWidget(/x/i.test(state.doc.sliceString(n.from, n.to))) })); return false; }
        if (name === "Image") {
          if (act) return;
          const text = state.doc.sliceString(n.from, n.to);
          const m = /^!\[([^\]]*)\]\(([^)\s]+)/.exec(text);
          if (m) { push(n.from, n.to, Decoration.replace({ widget: new ImageWidget(m[2], m[1]) })); return false; }
          return;
        }
        if (name === "Link") {
          push(n.from, n.to, inline("cm-lp-link"));
          if (!act) {
            // hide everything except the link text: [text](url) -> text
            const text = state.doc.sliceString(n.from, n.to);
            const m = /^\[([^\]]*)\]\(.*\)$/s.exec(text);
            if (m) { push(n.from, n.from + 1, hidden); push(n.from + 1 + m[1].length, n.to, hidden); return false; }
          }
          return;
        }
        if (MARKS.has(name)) {
          if (name === "ListMark") return; // keep bullets visible
          push(n.from, n.to, act ? mark : hidden);
          return false;
        }
        if (name === "CodeInfo" && !act) { push(n.from, n.to, hidden); return false; }
      },
    });
  }
  ranges.sort((a, b) => a.from - b.from || (a.deco.spec.block ? -1 : 0) || startSideOf(a.deco) - startSideOf(b.deco));
  const b = new RangeSetBuilder<Decoration>();
  for (const r of ranges) b.add(r.from, r.to, r.deco);
  return b.finish();
}
function startSideOf(d: Decoration) { return (d as any).startSide ?? 0; }

export const livePreview: Extension = ViewPlugin.fromClass(class {
  decorations: DecorationSet;
  constructor(view: EditorView) { this.decorations = buildDecorations(view); }
  update(u: ViewUpdate) { if (u.docChanged || u.viewportChanged || u.selectionSet) this.decorations = buildDecorations(u.view); }
}, { decorations: (v) => v.decorations });

