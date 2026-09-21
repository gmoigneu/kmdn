import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowUp, Bot, Pencil } from "lucide-react";
import { api, type AgentKind } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";

export function Home() {
  const { kb, go } = useUi();
  const root = kb!.root;
  const qc = useQueryClient();
  const [text, setText] = useState("");
  const [mode, setMode] = useState<"suggest" | "edit">("edit");
  const detected = useQuery({ queryKey: ["agents"], queryFn: api.agentDetect, staleTime: 60_000 });
  const [agent, setAgent] = useState<AgentKind | "">("");
  const chosenAgent = (agent || detected.data?.find((d) => d.available)?.kind || "") as AgentKind | "";
  const threads = useQuery({ queryKey: ["threads", root], queryFn: () => api.listThreads(root) });
  const reviews = useQuery({ queryKey: ["reviews", root], queryFn: () => api.listReviews(root), enabled: kb!.authenticated, retry: false });
  const prFor = (branch: string) => reviews.data?.find((p) => p.head_branch === branch);
  const live = threads.data?.filter((t) => !t.merged_at) ?? [];
  const groups = [
    { title: "In review", items: live.filter((t) => prFor(t.branch)) },
    { title: "Draft", items: live.filter((t) => !prFor(t.branch)) },
  ];
  const done = threads.data?.filter((t) => t.merged_at) ?? [];
  const [showDone, setShowDone] = useState(false);
  const create = useMutation({
    mutationFn: (slug: string) => api.createThread(root, slug),
    onSuccess: (t) => {
      qc.invalidateQueries({ queryKey: ["threads", root] });
      const prompt = text.trim();
      setText("");
      // The composer text is the first message to the agent, not just a name (06-agents.md).
      go({ kind: "thread", slug: t.slug, initialMode: mode, initialPrompt: chosenAgent ? prompt : undefined, initialAgent: chosenAgent || undefined });
    },
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
            <label className="flex items-center gap-1 px-2 h-7 rounded-md border border-border text-fg-muted">
              <Bot size={12} />
              <select value={chosenAgent} onChange={(e) => setAgent(e.target.value as AgentKind | "")} className="bg-transparent outline-none text-fg">
                {!detected.data?.some((d) => d.available) && <option value="">No agent installed</option>}
                {detected.data?.filter((d) => d.available).map((d) => <option key={d.kind} value={d.kind}>{d.kind === "claude" ? "Claude" : d.kind === "codex" ? "Codex" : "pi"}</option>)}
              </select>
            </label>
            <div className="flex rounded-md border border-border overflow-hidden">
              {(["suggest", "edit"] as const).map((m) => (
                <button key={m} onClick={() => setMode(m)} className={cn("px-2 h-7 capitalize", mode === m ? "bg-bg-muted" : "text-fg-muted")}>{m}</button>
              ))}
            </div>
            <button disabled={!text.trim() || create.isPending} onClick={() => create.mutate(text.trim().split(/\s+/).slice(0, 6).join(" ").slice(0, 48))}
              className="ml-auto size-7 rounded-md bg-accent text-accent-fg grid place-items-center disabled:opacity-40" title="Start thread">
              <ArrowUp size={14} />
            </button>
          </div>
        </div>
        {create.error && <p className="mt-2 text-danger text-xs">{String(create.error)}</p>}

        {groups.map((g) => g.items.length > 0 && (
          <div key={g.title}>
            <h2 className="mt-10 text-[11px] uppercase tracking-wide text-fg-muted">{g.title}</h2>
            <ul className="mt-2 divide-y divide-border rounded-md border border-border">
              {g.items.map((t) => {
                const p = prFor(t.branch);
                return (
                  <li key={t.slug}>
                    <button onClick={() => go({ kind: "thread", slug: t.slug })} className="w-full flex items-center gap-3 px-3 py-2 hover:bg-bg-muted text-left">
                      <Pencil size={14} className={p ? "text-accent" : "text-fg-muted"} />
                      <span className="flex-1 truncate">{p?.title ?? t.slug}</span>
                      {p && <span className="text-[10px] text-fg-muted">#{p.number}</span>}
                      <span className="font-mono text-[10px] text-fg-muted truncate max-w-[35%]">{t.branch}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
        {done.length > 0 && (
          <div>
            <button onClick={() => setShowDone((d) => !d)} className="mt-10 text-[11px] uppercase tracking-wide text-fg-muted hover:text-fg">Done · {done.length} {showDone ? "▾" : "▸"}</button>
            {showDone && (
              <ul className="mt-2 divide-y divide-border rounded-md border border-border opacity-70">
                {done.map((t) => (
                  <li key={t.slug}>
                    <button onClick={() => go({ kind: "thread", slug: t.slug })} className="w-full flex items-center gap-3 px-3 py-2 hover:bg-bg-muted text-left">
                      <Pencil size={14} className="text-ok" />
                      <span className="flex-1 truncate">{t.slug}</span>
                      <span className="text-[10px] text-fg-muted">published {new Date(t.merged_at! * 1000).toLocaleDateString()}, removed after 7 days</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
        {!threads.data?.length && <p className="mt-10 text-fg-muted text-xs">No threads yet. Describe a change above, or open a document and press Edit.</p>}
      </div>
    </div>
  );
}
