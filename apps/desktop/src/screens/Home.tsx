import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowUp, Bot, Pencil } from "lucide-react";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";

export function Home() {
  const { kb, go } = useUi();
  const root = kb!.root;
  const qc = useQueryClient();
  const [text, setText] = useState("");
  const [mode, setMode] = useState<"suggest" | "edit">("edit");
  const threads = useQuery({ queryKey: ["threads", root], queryFn: () => api.listThreads(root) });
  const create = useMutation({
    mutationFn: (slug: string) => api.createThread(root, "me", slug),
    onSuccess: (t) => { qc.invalidateQueries({ queryKey: ["threads", root] }); setText(""); go({ kind: "thread", slug: t.slug }); },
  });

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="max-w-[760px] mx-auto px-6 pt-16">
        <h1 className="text-xl font-medium">What should we change?</h1>
        <div className="mt-4 rounded-md border border-border bg-bg-elevated">
          <textarea value={text} onChange={(e) => setText(e.target.value)} rows={3}
            placeholder="Describe the change, or start a thread and edit by hand."
            className="w-full resize-none bg-transparent p-3 outline-none text-sm" />
          <div className="flex items-center gap-2 px-2 py-2 border-t border-border text-xs">
            <span className="flex items-center gap-1 px-2 h-7 rounded-md border border-border text-fg-muted"><Bot size={12} /> No agent</span>
            <div className="flex rounded-md border border-border overflow-hidden">
              {(["suggest", "edit"] as const).map((m) => (
                <button key={m} onClick={() => setMode(m)} className={cn("px-2 h-7 capitalize", mode === m ? "bg-bg-muted" : "text-fg-muted")}>{m}</button>
              ))}
            </div>
            <button disabled={!text.trim() || create.isPending} onClick={() => create.mutate(text.trim().slice(0, 48))}
              className="ml-auto size-7 rounded-md bg-accent text-accent-fg grid place-items-center disabled:opacity-40" title="Start thread">
              <ArrowUp size={14} />
            </button>
          </div>
        </div>
        {create.error && <p className="mt-2 text-danger text-xs">{String(create.error)}</p>}

        <h2 className="mt-10 text-[11px] uppercase tracking-wide text-fg-muted">Draft</h2>
        <ul className="mt-2 divide-y divide-border rounded-md border border-border">
          {threads.data?.map((t) => (
            <li key={t.slug}>
              <button onClick={() => go({ kind: "thread", slug: t.slug })} className="w-full flex items-center gap-3 px-3 py-2 hover:bg-bg-muted text-left">
                <Pencil size={14} className="text-fg-muted" />
                <span className="flex-1 truncate">{t.slug}</span>
                <span className="font-mono text-[10px] text-fg-muted truncate max-w-[40%]">{t.branch}</span>
              </button>
            </li>
          ))}
          {!threads.data?.length && <li className="px-3 py-3 text-fg-muted text-xs">No threads yet.</li>}
        </ul>
      </div>
    </div>
  );
}
