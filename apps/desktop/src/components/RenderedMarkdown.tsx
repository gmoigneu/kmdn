// Read view: GFM rendered with the same block renderer used by the diff (D17: one parser everywhere).
import { Fragment, useMemo } from "react";
import { marked } from "marked";
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

export function RenderedMarkdown({ text, className }: { text: string; className?: string }) {
  const { keys, body } = useMemo(() => fm(text), [text]);
  const html = useMemo(() => marked.parse(body, { async: false, gfm: true }) as string, [body]);
  return (
    <article className={cn("prose-pane", className)}>
      {keys.length > 0 && (
        <dl className="mb-6 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs text-fg-muted border border-border rounded-md p-3">
          {keys.map(([k, v], i) => (<Fragment key={`${k}-${i}`}><dt className="font-mono">{k}</dt><dd className="text-fg">{v}</dd></Fragment>))}
        </dl>
      )}
      <div className="rendered" dangerouslySetInnerHTML={{ __html: html }} />
    </article>
  );
}
