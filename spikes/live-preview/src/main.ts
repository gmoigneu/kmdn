import { EditorView, keymap, lineNumbers, highlightActiveLine } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
import { livePreview } from "./livePreview";
import { sample } from "./sample";

const view = new EditorView({
  state: EditorState.create({
    doc: sample(2000),
    extensions: [
      history(), keymap.of([...defaultKeymap, ...historyKeymap]),
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      syntaxHighlighting(defaultHighlightStyle),
      highlightActiveLine(), EditorView.lineWrapping,
      livePreview,
    ],
  }),
  parent: document.getElementById("editor")!,
});
(window as any).view = view; (window as any).lineNumbers = lineNumbers;
