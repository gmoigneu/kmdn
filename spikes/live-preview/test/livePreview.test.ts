import { EditorView } from "@codemirror/view";
import { EditorState, EditorSelection } from "@codemirror/state";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { buildDecorations, frontmatterRange } from "../src/livePreview";
import { sample } from "../src/sample";

function make(doc: string, cursor = 0) {
  const view = new EditorView({
    state: EditorState.create({ doc, selection: EditorSelection.cursor(cursor), extensions: [markdown({ base: markdownLanguage })] }),
    parent: document.body,
  });
  // jsdom has no layout: parse eagerly and pretend the whole document is visible.
  ensureSyntaxTree(view.state, doc.length, 5000);
  Object.defineProperty(view, "visibleRanges", { get: () => [{ from: 0, to: doc.length }] });
  return view;
}

function decos(view: EditorView) {
  const set = buildDecorations(view);
  const out: { from: number; to: number; kind: string; cls?: string; widget?: string }[] = [];
  set.between(0, view.state.doc.length, (from, to, d) => {
    const spec: any = d.spec;
    out.push({ from, to, kind: spec.widget ? "widget" : spec.class ? "class" : "replace", cls: spec.class, widget: spec.widget?.constructor?.name });
  });
  return out;
}

describe("live preview decorations", () => {
  test("frontmatter is detected and collapsed when cursor is outside", () => {
    const doc = "---\ntitle: X\nstatus: draft\n---\n# Hello\n";
    expect(frontmatterRange(doc)).toEqual([0, 30]);
    const d = decos(make(doc, doc.length - 1));
    const fm = d.find((x) => x.widget === "FrontmatterWidget");
    expect(fm).toBeDefined();
    expect(fm!.from).toBe(0);
  });

  test("frontmatter expands when cursor is inside", () => {
    const doc = "---\ntitle: X\n---\n# Hello\n";
    const d = decos(make(doc, 6));
    expect(d.find((x) => x.widget === "FrontmatterWidget")).toBeUndefined();
    expect(d.some((x) => x.cls === "cm-lp-fm")).toBe(true);
  });

  test("markers hidden on inactive lines, visible on the active line", () => {
    const doc = "# Title\n\nSome **bold** and *em* here.\n";
    let d = decos(make(doc, 2));
    expect(d.some((x) => x.from === 0 && x.to === 1 && x.cls === "cm-lp-mark")).toBe(true);
    const boldStart = doc.indexOf("**");
    expect(d.some((x) => x.from === boldStart && x.to === boldStart + 2 && x.kind === "replace")).toBe(true);
    d = decos(make(doc, boldStart + 3));
    expect(d.some((x) => x.from === boldStart && x.to === boldStart + 2 && x.cls === "cm-lp-mark")).toBe(true);
  });

  test("headings, emphasis, code, link, quote, table, image, task, hr all decorated", () => {
    const doc = sample(60);
    const d = decos(make(doc, doc.length - 1));
    const classes = new Set(d.map((x) => x.cls).filter(Boolean));
    for (const c of ["cm-lp-h1", "cm-lp-h2", "cm-lp-strong", "cm-lp-em", "cm-lp-code", "cm-lp-link", "cm-lp-quote", "cm-lp-fence", "cm-lp-table", "cm-lp-strike"]) {
      expect(classes.has(c), c).toBe(true);
    }
    const widgets = new Set(d.map((x) => x.widget).filter(Boolean));
    for (const w of ["ImageWidget", "TaskWidget", "HrWidget", "FrontmatterWidget"]) expect(widgets.has(w), w).toBe(true);
  });

  test("link keeps only its text visible when inactive", () => {
    const doc = "See [rollback](rollback.md) now.\n\nOther line.\n";
    const d = decos(make(doc, doc.length - 2));
    const start = doc.indexOf("[");
    expect(d.some((x) => x.from === start && x.to === start + 1 && x.kind === "replace")).toBe(true);
    expect(d.some((x) => x.from === start + 9 && x.kind === "replace")).toBe(true);
  });

  test("decoration build for a 2,000-line document is fast", () => {
    const doc = sample(2000);
    const view = make(doc, doc.length - 1);
    const t0 = performance.now();
    for (let i = 0; i < 10; i++) buildDecorations(view);
    const per = (performance.now() - t0) / 10;
    // CPU budget for one rebuild over the WHOLE document; the real editor only decorates visible ranges.
    expect(per).toBeLessThan(120);
    console.log(`build over full 2000-line doc: ${per.toFixed(1)} ms avg`);
  });
});
