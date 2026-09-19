// Live-preview markdown editor (D16). CodeMirror 6 with the decoration plugin from spike 1.
import { useEffect, useRef } from "react";
import { EditorView, keymap, highlightActiveLine, drawSelection } from "@codemirror/view";
import { EditorState, Compartment } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
import { livePreview } from "@/lib/editor/livePreview";

interface Props {
  value: string;
  onChange: (next: string) => void;
  onSave?: () => void;
  className?: string;
}

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "15px", lineHeight: "1.65", backgroundColor: "transparent", color: "var(--fg)" },
  ".cm-scroller": { fontFamily: "var(--font-ui)", padding: "24px 0" },
  ".cm-content": { maxWidth: "var(--measure)", margin: "0 auto", padding: "0 24px", caretColor: "var(--accent)" },
  ".cm-line": { padding: "0" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--fg) 4%, transparent)" },
  "&.cm-focused": { outline: "none" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": { backgroundColor: "color-mix(in srgb, var(--accent) 25%, transparent)" },
  ".cm-lp-h1": { fontSize: "1.8em", fontWeight: "600", lineHeight: "1.3" },
  ".cm-lp-h2": { fontSize: "1.45em", fontWeight: "600" },
  ".cm-lp-h3": { fontSize: "1.2em", fontWeight: "600" },
  ".cm-lp-mark": { color: "var(--fg-muted)", opacity: "0.6" },
  ".cm-lp-em": { fontStyle: "italic" },
  ".cm-lp-strong": { fontWeight: "700" },
  ".cm-lp-strike": { textDecoration: "line-through" },
  ".cm-lp-code": { fontFamily: "var(--font-mono)", fontSize: "0.9em", backgroundColor: "var(--bg-muted)", borderRadius: "4px", padding: "0 0.25em" },
  ".cm-lp-fence": { fontFamily: "var(--font-mono)", fontSize: "0.9em", backgroundColor: "var(--bg-muted)", borderLeft: "2px solid var(--border)" },
  ".cm-lp-link": { color: "var(--accent)", textDecoration: "underline" },
  ".cm-lp-quote": { borderLeft: "3px solid var(--border)", color: "var(--fg-muted)", paddingLeft: "0.75em" },
  ".cm-lp-img": { maxWidth: "100%", display: "block", margin: "0.5em 0", borderRadius: "6px" },
  ".cm-lp-task": { marginRight: "0.4em", verticalAlign: "middle" },
  ".cm-lp-fm": { color: "var(--fg-muted)", fontFamily: "var(--font-mono)", fontSize: "0.85em", backgroundColor: "var(--bg-muted)" },
  ".cm-lp-fm-collapsed": { color: "var(--fg-muted)", fontSize: "0.85em", cursor: "pointer", border: "1px solid var(--border)", borderRadius: "6px", padding: "2px 8px" },
  ".cm-lp-table": { fontFamily: "var(--font-mono)", fontSize: "0.9em" },
  ".cm-lp-hr": { display: "block", borderTop: "1px solid var(--border)", margin: "0.5em 0" },
});

export function MarkdownEditor({ value, onChange, onSave, className }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const saveCompartment = useRef(new Compartment());
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  onChangeRef.current = onChange;
  onSaveRef.current = onSave;

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      state: EditorState.create({
        doc: value,
        extensions: [
          history(),
          drawSelection(),
          highlightActiveLine(),
          EditorView.lineWrapping,
          keymap.of([
            { key: "Mod-s", run: () => { onSaveRef.current?.(); return true; } },
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          markdown({ base: markdownLanguage, codeLanguages: languages }),
          syntaxHighlighting(defaultHighlightStyle),
          livePreview,
          theme,
          saveCompartment.current.of([]),
          EditorView.updateListener.of((u) => { if (u.docChanged) onChangeRef.current(u.state.doc.toString()); }),
        ],
      }),
      parent: host.current,
    });
    view.current = v;
    return () => { v.destroy(); view.current = null; };
    // Mount once per document; external value changes are pushed below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const current = v.state.doc.toString();
    if (current !== value) {
      v.dispatch({ changes: { from: 0, to: current.length, insert: value } });
    }
  }, [value]);

  return <div ref={host} className={className} />;
}
