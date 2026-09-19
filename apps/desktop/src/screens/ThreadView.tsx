import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";

type Tab = "changes" | "editor" | "read";

export function ThreadView({ slug }: { slug: string }) {
  const { kb } = useUi();
  const root = kb!.root;
  const threads = useQuery({ queryKey: ["threads", root], queryFn: () => api.listThreads(root) });
  const t = threads.data?.find((x) => x.slug === slug);
  const [tab, setTab] = useState<Tab>("changes");
  const docs = useQuery({ queryKey: ["docs", t?.path], queryFn: () => api.listDocuments(t!.path), enabled: !!t });

  if (!t) return <div className="p-6 text-fg-muted">Loading thread…</div>;
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <header className="h-11 shrink-0 border-b border-border flex items-center gap-3 px-4">
        <h1 className="font-medium truncate">{t.slug}</h1>
        <span className="text-[10px] px-1.5 rounded-full border border-border text-fg-muted">draft</span>
        <span className="ml-auto" />
        <button disabled className="h-7 px-3 rounded-md bg-accent text-accent-fg text-xs disabled:opacity-40" title="Submit lands in #27">Submit for review</button>
      </header>
      <div className="flex-1 flex min-h-0">
        <section className="w-[42%] min-w-[320px] border-r border-border flex flex-col">
          <div className="flex-1 overflow-y-auto p-4 text-fg-muted text-xs">
            Timeline. Agent turns and saves show up here (#24).
            <div className="mt-3 font-mono text-[10px] break-all">{t.branch}<br />{t.path}</div>
          </div>
          <div className="border-t border-border p-2">
            <textarea rows={2} placeholder="Ask an agent or leave a note…" className="w-full resize-none rounded-md border border-border bg-bg p-2 text-sm outline-none" />
          </div>
        </section>
        <section className="flex-1 flex flex-col min-w-0">
          <div className="h-9 border-b border-border flex items-center px-2 gap-1 text-xs">
            {(["changes", "editor", "read"] as Tab[]).map((x) => (
              <button key={x} onClick={() => setTab(x)} className={cn("px-2 h-7 rounded-md capitalize", tab === x ? "bg-bg-muted" : "text-fg-muted")}>{x}</button>
            ))}
          </div>
          <div className="flex-1 overflow-y-auto p-4">
            {tab === "changes" && <p className="text-fg-muted text-xs">No changes yet. Rendered diff lands in #26.</p>}
            {tab === "editor" && <p className="text-fg-muted text-xs">Live-preview editor lands in #25.</p>}
            {tab === "read" && (
              <ul className="text-xs space-y-1">{docs.data?.map((d) => <li key={d.path} className="font-mono">{d.path}</li>)}</ul>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
