// Live-preview markdown editor (D16). CodeMirror 6 with the decoration plugin from spike 1.
import { useEffect, useRef } from "react";
import { EditorView, keymap, highlightActiveLine, drawSelection } from "@codemirror/view";
import { EditorState, Compartment } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { livePreview } from "@/lib/editor/livePreview";

interface Props {
  value: string;
  onChange: (next: string) => void;
  onSave?: () => void;
  /** Pasted or dropped image: return the relative link to insert, or null to ignore. */
  onImage?: (file: File) => Promise<string | null>;
  /** Mod-Shift-L: open the link picker. */
  onLinkPicker?: () => void;
  /** Receives a function that inserts text at the cursor. */
  registerInsert?: (insert: (text: string) => void) => void;
  className?: string;
}

/** Code fences follow the theme through the --syn-* tokens (lib/appearance.ts). */
const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.definitionKeyword], color: "var(--syn-keyword)" },
  { tag: [t.string, t.special(t.string), t.regexp], color: "var(--syn-string)" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment, t.meta], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--syn-number)" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.definition(t.variableName)], color: "var(--syn-function)" },
  { tag: [t.typeName, t.className, t.namespace, t.tagName], color: "var(--syn-type)" },
  { tag: [t.propertyName, t.attributeName, t.labelName], color: "var(--syn-property)" },
  { tag: [t.operator, t.punctuation, t.separator], color: "var(--syn-operator)" },
]);

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "var(--reading-size)", lineHeight: "1.7", backgroundColor: "transparent", color: "var(--fg)" },
  ".cm-scroller": { fontFamily: "var(--font-prose)", padding: "24px 0" },
  ".cm-content": { maxWidth: "var(--measure)", margin: "0 auto", padding: "0 24px", caretColor: "var(--accent)" },
  // One markdown block per line: the padding is the paragraph and list-item spacing (#77).
  ".cm-line": { padding: "0.15em 0" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--fg) 4%, transparent)" },
  "&.cm-focused": { outline: "none" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": { backgroundColor: "color-mix(in srgb, var(--accent) 25%, transparent)" },
  ".cm-lp-h1": { fontSize: "1.8em", fontWeight: "600", lineHeight: "1.3", paddingTop: "0.7em", paddingBottom: "0.2em" },
  ".cm-lp-h2": { fontSize: "1.45em", fontWeight: "600", lineHeight: "1.3", paddingTop: "0.9em", paddingBottom: "0.2em" },
  ".cm-lp-h3": { fontSize: "1.2em", fontWeight: "600", lineHeight: "1.35", paddingTop: "0.8em", paddingBottom: "0.15em" },
  ".cm-lp-mark": { color: "var(--fg-muted)", opacity: "0.6" },
  ".cm-lp-em": { fontStyle: "italic" },
  ".cm-lp-strong": { fontWeight: "700" },
  ".cm-lp-strike": { textDecoration: "line-through" },
  ".cm-lp-code": { fontFamily: "var(--font-mono)", fontSize: "0.9em", backgroundColor: "var(--bg-muted)", borderRadius: "4px", padding: "0 0.25em" },
  ".cm-lp-fence": { fontFamily: "var(--font-mono)", fontSize: "0.9em", lineHeight: "1.6", backgroundColor: "var(--bg-muted)", borderLeft: "2px solid var(--border)", paddingLeft: "0.75em", paddingRight: "0.75em" },
  ".cm-lp-link": { color: "var(--accent)", textDecoration: "underline" },
  ".cm-lp-quote": { borderLeft: "3px solid var(--border)", color: "var(--fg-muted)", paddingLeft: "1em" },
  ".cm-lp-img": { maxWidth: "100%", display: "block", margin: "0.5em 0", borderRadius: "6px" },
  ".cm-lp-task": { marginRight: "0.4em", verticalAlign: "middle" },
  ".cm-lp-fm": { color: "var(--fg-muted)", fontFamily: "var(--font-mono)", fontSize: "0.85em", backgroundColor: "var(--bg-muted)" },
  ".cm-lp-fm-collapsed": { color: "var(--fg-muted)", fontSize: "0.85em", cursor: "pointer", border: "1px solid var(--border)", borderRadius: "6px", padding: "2px 8px" },
  ".cm-lp-table": { fontFamily: "var(--font-mono)", fontSize: "0.9em" },
  ".cm-lp-hr": { display: "block", borderTop: "1px solid var(--border)", margin: "0.5em 0" },
});

export function MarkdownEditor({ value, onChange, onSave, onImage, onLinkPicker, registerInsert, className }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const saveCompartment = useRef(new Compartment());
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  const onImageRef = useRef(onImage);
  const onLinkRef = useRef(onLinkPicker);
  onChangeRef.current = onChange;
  onSaveRef.current = onSave;
  onImageRef.current = onImage;
  onLinkRef.current = onLinkPicker;

  const insertAtCursor = (v: EditorView, text: string) => {
    const { from, to } = v.state.selection.main;
    v.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length } });
    v.focus();
  };

  /** Images from the clipboard or a drop land in assets/ next to the document (D18). */
  const handleFiles = async (v: EditorView, files: FileList | null | undefined): Promise<boolean> => {
    if (!files || !onImageRef.current) return false;
    const images = Array.from(files).filter((f) => f.type.startsWith("image/"));
    if (images.length === 0) return false;
    for (const f of images) {
      const rel = await onImageRef.current(f);
      if (rel) insertAtCursor(v, `![${f.name.replace(/\.[^.]+$/, "")}](${rel})\n`);
    }
    return true;
  };

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
          EditorView.domEventHandlers({
            paste: (e, v) => {
              const files = e.clipboardData?.files;
              if (files && files.length > 0 && Array.from(files).some((f) => f.type.startsWith("image/"))) { e.preventDefault(); void handleFiles(v, files); return true; }
              return false;
            },
            drop: (e, v) => {
              const files = e.dataTransfer?.files;
              if (files && files.length > 0 && Array.from(files).some((f) => f.type.startsWith("image/"))) { e.preventDefault(); void handleFiles(v, files); return true; }
              return false;
            },
          }),
          keymap.of([
            { key: "Mod-s", run: () => { onSaveRef.current?.(); return true; } },
            { key: "Mod-Shift-l", run: () => { onLinkRef.current?.(); return true; } },
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          markdown({ base: markdownLanguage, codeLanguages: languages }),
          syntaxHighlighting(highlight),
          livePreview,
          theme,
          saveCompartment.current.of([]),
          EditorView.updateListener.of((u) => { if (u.docChanged) onChangeRef.current(u.state.doc.toString()); }),
        ],
      }),
      parent: host.current,
    });
    view.current = v;
    registerInsert?.((text) => insertAtCursor(v, text));
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
