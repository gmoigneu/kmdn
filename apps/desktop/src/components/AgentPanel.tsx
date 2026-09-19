// Thread timeline and composer (10-ui.md: Thread, Composer controls). One agent session per thread.
import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { ArrowUp, Bot, Check, Loader2, Square, X } from "lucide-react";
import { api, type AgentEnvelope, type AgentEvent, type AgentKind, type AgentMode, type ToolKind } from "@/lib/api";
import { cn } from "@/lib/utils";

type Item =
  | { kind: "user"; text: string }
  | { kind: "assistant"; text: string; streaming: boolean }
  | { kind: "tool"; id: string; tool: ToolKind; paths: string[]; command: string | null; ok: boolean | null; summary: string }
  | { kind: "permission"; id: string; tool: ToolKind; paths: string[]; command: string | null; answered: boolean | null }
  | { kind: "note"; text: string; error?: boolean };

const AGENT_LABEL: Record<AgentKind, string> = { claude: "Claude", codex: "Codex", pi: "pi" };

function toolLabel(t: ToolKind) {
  return typeof t === "string" ? t : t.other;
}

export function AgentPanel({ root, slug, branch, onChanged }: { root: string; slug: string; branch: string; onChanged: () => void }) {
  const detected = useQuery({ queryKey: ["agents"], queryFn: api.agentDetect, staleTime: 60_000 });
  const session = useQuery({ queryKey: ["agent-session", slug], queryFn: () => api.agentSession(slug) });
  const [kind, setKind] = useState<AgentKind>("claude");
  const [mode, setMode] = useState<AgentMode>("edit");
  const [text, setText] = useState("");
  const [items, setItems] = useState<Item[]>([]);
  const [busy, setBusy] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const first = detected.data?.find((d) => d.available)?.kind;
    if (first && !detected.data?.find((d) => d.kind === kind)?.available) setKind(first);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detected.data]);

  useEffect(() => {
    const un = listen<AgentEnvelope>("agent://event", (e) => {
      if (e.payload.slug !== slug) return;
      apply(e.payload.event);
    });
    return () => { un.then((f) => f()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [slug]);

  useEffect(() => { scroller.current?.scrollTo({ top: scroller.current.scrollHeight }); }, [items]);

  function apply(ev: AgentEvent) {
    setItems((prev) => {
      const next = [...prev];
      const last = next[next.length - 1];
      switch (ev.type) {
        case "text_delta":
          if (last?.kind === "assistant" && last.streaming) next[next.length - 1] = { ...last, text: last.text + ev.text };
          else next.push({ kind: "assistant", text: ev.text, streaming: true });
          return next;
        case "text":
          if (last?.kind === "assistant" && last.streaming) { next[next.length - 1] = { ...last, text: ev.text, streaming: false }; return next; }
          next.push({ kind: "assistant", text: ev.text, streaming: false });
          return next;
        case "tool_call_started":
          if (last?.kind === "assistant") next[next.length - 1] = { ...last, streaming: false };
          next.push({ kind: "tool", id: ev.id, tool: ev.kind, paths: ev.paths, command: ev.command, ok: null, summary: "" });
          return next;
        case "tool_call_finished": {
          const i = next.findIndex((x) => x.kind === "tool" && x.id === ev.id);
          if (i >= 0) next[i] = { ...(next[i] as Extract<Item, { kind: "tool" }>), ok: ev.ok, summary: ev.summary };
          else if (ev.summary && ev.summary !== "auto-approved") next.push({ kind: "note", text: ev.summary, error: !ev.ok });
          return next;
        }
        case "permission_request":
          next.push({ kind: "permission", id: ev.id, tool: ev.kind, paths: ev.paths, command: ev.command, answered: null });
          return next;
        case "turn_done":
          setBusy(false);
          onChanged();
          if (last?.kind === "assistant") next[next.length - 1] = { ...last, streaming: false };
          return next;
        case "error":
          setBusy(false);
          next.push({ kind: "note", text: ev.message, error: true });
          return next;
        case "session_started":
          return next;
      }
    });
  }

  const start = useMutation({
    mutationFn: () => api.agentStart(root, slug, kind, mode, null),
    onSuccess: () => { session.refetch(); setItems((p) => [...p, { kind: "note", text: `${AGENT_LABEL[kind]} started in ${mode} mode.` }]); },
    onError: (e) => setItems((p) => [...p, { kind: "note", text: String(e), error: true }]),
  });
  const send = useMutation({
    mutationFn: async () => {
      const t = text.trim();
      if (!session.data) await start.mutateAsync();
      setItems((p) => [...p, { kind: "user", text: t }]);
      setText("");
      setBusy(true);
      await api.agentSend(slug, t);
    },
    onError: (e) => { setBusy(false); setItems((p) => [...p, { kind: "note", text: String(e), error: true }]); },
  });
  const answer = useMutation({
    mutationFn: ({ id, allow }: { id: string; allow: boolean }) => api.agentReplyPermission(slug, id, allow, allow ? null : "denied by user"),
    onSuccess: (_, { id, allow }) => setItems((p) => p.map((x) => (x.kind === "permission" && x.id === id ? { ...x, answered: allow } : x))),
  });

  const available = useMemo(() => detected.data?.filter((d) => d.available) ?? [], [detected.data]);
  const noAgents = detected.data && available.length === 0;

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div ref={scroller} className="flex-1 overflow-y-auto p-4 space-y-3 text-sm">
        {items.length === 0 && (
          <div className="text-xs text-fg-muted space-y-1">
            <p>Ask an agent to change documents in this thread, or edit by hand in the Editor tab.</p>
            <p className="font-mono text-[10px] break-all opacity-70">{branch}</p>
            {noAgents && <p>No agent CLI found on PATH. Install Claude Code, Codex, or pi and sign in to it, then reopen this thread.</p>}
          </div>
        )}
        {items.map((it, i) => {
          switch (it.kind) {
            case "user":
              return <div key={i} className="ml-8 rounded-md bg-bg-muted px-3 py-2 whitespace-pre-wrap">{it.text}</div>;
            case "assistant":
              return <div key={i} className={cn("rounded-md border border-border px-3 py-2 whitespace-pre-wrap", it.streaming && "border-accent/40")}>{it.text}{it.streaming && <span className="inline-block w-1.5 h-4 bg-accent align-middle ml-0.5 animate-pulse" />}</div>;
            case "tool":
              return (
                <div key={i} className="flex items-start gap-2 text-xs text-fg-muted px-1">
                  {it.ok === null ? <Loader2 size={12} className="mt-0.5 animate-spin" /> : it.ok ? <Check size={12} className="mt-0.5 text-ok" /> : <X size={12} className="mt-0.5 text-danger" />}
                  <div className="min-w-0">
                    <span className="font-mono">{toolLabel(it.tool)}</span>{" "}
                    <span className="font-mono break-all">{it.command ?? it.paths.map((p) => p.split("/").slice(-2).join("/")).join(", ")}</span>
                    {it.ok === false && it.summary && <div className="text-danger/80 mt-0.5">{it.summary}</div>}
                  </div>
                </div>
              );
            case "permission":
              return (
                <div key={i} className="rounded-md border border-warn/50 bg-bg-muted px-3 py-2 text-xs">
                  <div>{AGENT_LABEL[kind]} wants to {toolLabel(it.tool) === "shell" ? "run" : toolLabel(it.tool)}: <span className="font-mono break-all">{it.command ?? it.paths.join(", ")}</span></div>
                  {it.answered === null ? (
                    <div className="mt-2 flex gap-2">
                      <button onClick={() => answer.mutate({ id: it.id, allow: true })} className="h-6 px-2 rounded-md bg-accent text-accent-fg">Allow</button>
                      <button onClick={() => answer.mutate({ id: it.id, allow: false })} className="h-6 px-2 rounded-md border border-border">Deny</button>
                    </div>
                  ) : <div className="mt-1 text-fg-muted">{it.answered ? "Allowed" : "Denied"}</div>}
                </div>
              );
            case "note":
              return <div key={i} className={cn("text-xs px-1", it.error ? "text-danger" : "text-fg-muted")}>{it.text}</div>;
          }
        })}
      </div>
      <div className="border-t border-border p-2 space-y-2">
        <textarea value={text} onChange={(e) => setText(e.target.value)} rows={3}
          onKeyDown={(e) => { if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && text.trim() && !busy) send.mutate(); }}
          placeholder={noAgents ? "No agent available. Edit by hand in the Editor tab." : `Ask ${AGENT_LABEL[kind]}… (⌘↵ to send)`}
          disabled={!!noAgents}
          className="w-full resize-none rounded-md border border-border bg-bg p-2 text-sm outline-none disabled:opacity-50" />
        <div className="flex items-center gap-2 text-xs">
          <select value={kind} onChange={(e) => setKind(e.target.value as AgentKind)} disabled={!!session.data}
            className="h-7 px-2 rounded-md border border-border bg-bg text-fg disabled:opacity-60" title={session.data ? "Agent is fixed for this thread session" : "Agent"}>
            {(["claude", "codex", "pi"] as AgentKind[]).map((k) => {
              const d = detected.data?.find((x) => x.kind === k);
              return <option key={k} value={k} disabled={!d?.available}>{AGENT_LABEL[k]}{d?.available ? "" : " (not installed)"}</option>;
            })}
          </select>
          <div className="flex rounded-md border border-border overflow-hidden">
            {(["suggest", "edit"] as AgentMode[]).map((m) => (
              <button key={m} onClick={() => setMode(m)} disabled={!!session.data} className={cn("px-2 h-7 capitalize", mode === m ? "bg-bg-muted" : "text-fg-muted")}>{m}</button>
            ))}
          </div>
          <span className="flex items-center gap-1 text-fg-muted"><Bot size={12} />{session.data ? `${AGENT_LABEL[session.data.kind]} · ${session.data.mode}` : "not started"}</span>
          <span className="ml-auto" />
          {busy && <button onClick={() => api.agentCancel(slug)} className="h-7 px-2 rounded-md border border-border flex items-center gap-1" title="Stop this turn"><Square size={11} /> Stop</button>}
          <button disabled={!text.trim() || busy || !!noAgents} onClick={() => send.mutate()} className="size-7 rounded-md bg-accent text-accent-fg grid place-items-center disabled:opacity-40" title="Send">
            {busy ? <Loader2 size={14} className="animate-spin" /> : <ArrowUp size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
