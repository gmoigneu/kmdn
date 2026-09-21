// Read view: GFM rendered with the same block renderer used by the diff (D17: one parser everywhere).
import { Fragment, useMemo } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isExternalHref, renderMarkdown } from "@/lib/sanitize";
import { useUi } from "@/lib/store";
import { splitBlocks } from "@/lib/rdiff/blocks";
import { cn } from "@/lib/utils";

function fm(text: string): { keys: [string, string][]; body: string } {
  const blocks = splitBlocks(text);
  const first = blocks[0];
  if (!first || first.kind !== "frontmatter") return { keys: [], body: text };
  const keys = first.text.split("\n").slice(1, -1).filter((l) => l && !l.startsWith("#")).map((l) => {
    const i = l.indexOf(":");
    return [l.slice(0, i).trim(), l.slice(i + 1).trim()] as [string, string];
  });
  return { keys, body: text.slice(first.text.length) };
}

/** Links never navigate the webview: external ones open in the OS browser, relative .md links open the document. */
function useLinkHandler(basePath?: string) {
  const go = useUi((s) => s.go);
  return (e: React.MouseEvent<HTMLElement>) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (!href || href.startsWith("#")) return;
    if (isExternalHref(href)) { openUrl(href).catch(() => {}); return; }
    if (basePath) {
      const target = href.split(/[#?]/)[0];
      if (!target.endsWith(".md")) return;
      const parts = basePath.split("/").slice(0, -1);
      for (const seg of target.split("/")) {
        if (seg === "..") parts.pop();
        else if (seg && seg !== ".") parts.push(seg);
      }
      go({ kind: "document", path: parts.join("/") });
    }
  };
}

export function RenderedMarkdown({ text, className, basePath }: { text: string; className?: string; basePath?: string }) {
  const { keys, body } = useMemo(() => fm(text), [text]);
  const html = useMemo(() => renderMarkdown(body), [body]);
  const onClick = useLinkHandler(basePath);
  return (
    <article className={cn("prose-pane", className)}>
      {keys.length > 0 && (
        <dl className="mb-6 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs text-fg-muted border border-border rounded-md p-3">
          {keys.map(([k, v], i) => (<Fragment key={`${k}-${i}`}><dt className="font-mono">{k}</dt><dd className="text-fg">{v}</dd></Fragment>))}
        </dl>
      )}
      <div className="rendered" onClick={onClick} dangerouslySetInnerHTML={{ __html: html }} />
    </article>
  );
}
