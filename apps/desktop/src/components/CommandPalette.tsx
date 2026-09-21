// Cmd-K palette (D52): documents, threads, reviews, actions. Every action also has a mouse path.
import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ClipboardCopy, FileText, GitPullRequest, Layers, Palette, Plus, RefreshCw, Search } from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";
import { useAppearance } from "@/lib/appearance";

interface Entry { id: string; group: string; label: string; hint?: string; icon: typeof Layers; run: () => void }

export function CommandPalette() {
  const { kb, go, toggleSidebar } = useUi();
  const openAppearance = useAppearance((s) => s.setOpen);
  const root = kb?.root;
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [cursor, setCursor] = useState(0);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setOpen((o) => !o); setQ(""); setCursor(0); }
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const docs = useQuery({ queryKey: ["docs", root], queryFn: () => api.listDocuments(root!), enabled: !!root && open });
  const threads = useQuery({ queryKey: ["threads", root], queryFn: () => api.listThreads(root!), enabled: !!root && open });
  const reviews = useQuery({ queryKey: ["reviews", root], queryFn: () => api.listReviews(root!), enabled: !!root && open && !!kb?.authenticated, retry: false });
  const newThread = useMutation({
    mutationFn: (slug: string) => api.createThread(root!, slug),
    onSuccess: (t) => { qc.invalidateQueries({ queryKey: ["threads", root] }); go({ kind: "thread", slug: t.slug }); },
  });
  const sync = useMutation({ mutationFn: () => api.syncNow(root!), onSuccess: () => qc.invalidateQueries() });
  const addCi = useMutation({
    mutationFn: () => api.addCiCheck(root!),
    onSuccess: (t) => { qc.invalidateQueries({ queryKey: ["threads", root] }); go({ kind: "thread", slug: t.slug }); },
  });

  const entries = useMemo<Entry[]>(() => {
    if (!root) return [];
    const list: Entry[] = [
      { id: "a:new", group: "Actions", label: "New thread", hint: "start a change", icon: Plus, run: () => { const name = q.replace(/^new\s+/i, "").trim() || "change"; newThread.mutate(name); } },
      { id: "a:sync", group: "Actions", label: "Sync now", hint: "fetch, fast-forward, rebase threads", icon: RefreshCw, run: () => sync.mutate() },
      { id: "a:home", group: "Actions", label: "Go home", icon: Layers, run: () => go({ kind: "home" }) },
      { id: "a:sidebar", group: "Actions", label: "Toggle sidebar", icon: Layers, run: toggleSidebar },
      { id: "a:appearance", group: "Actions", label: "Appearance", hint: "theme, fonts, text size", icon: Palette, run: () => openAppearance(true) },
      { id: "a:ci", group: "Actions", label: "Add CI check to this knowledge base", hint: "opens a thread with the kmdn-cli check job (optional, D60)", icon: Plus, run: () => addCi.mutate() },
      { id: "a:diagnostics", group: "Actions", label: "Copy diagnostics", hint: "version, agents, last log lines; no document text", icon: ClipboardCopy, run: () => { api.diagnostics().then((t) => writeText(t)).catch(() => {}); } },
    ];
    for (const t of threads.data ?? []) list.push({ id: `t:${t.slug}`, group: "Threads", label: t.slug, hint: t.branch, icon: Layers, run: () => go({ kind: "thread", slug: t.slug }) });
    for (const p of reviews.data ?? []) list.push({ id: `r:${p.number}`, group: "Reviews", label: p.title, hint: `#${p.number} by ${p.author}`, icon: GitPullRequest, run: () => go({ kind: "review", number: p.number }) });
    for (const d of docs.data ?? []) list.push({ id: `d:${d.path}`, group: "Documents", label: d.title, hint: d.path, icon: FileText, run: () => go({ kind: "document", path: d.path }) });
    return list;
  }, [root, q, threads.data, reviews.data, docs.data, go, toggleSidebar, openAppearance, newThread, sync, addCi]);

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return entries.slice(0, 40);
    const score = (e: Entry) => {
      const hay = `${e.label} ${e.hint ?? ""} ${e.group}`.toLowerCase();
      if (hay.startsWith(needle)) return 3;
      if (e.label.toLowerCase().includes(needle)) return 2;
      if (hay.includes(needle)) return 1;
      // subsequence match for fuzzy typing
      let i = 0; for (const ch of hay) { if (ch === needle[i]) i++; if (i === needle.length) return 0.5; }
      return 0;
    };
    return entries.map((e) => [e, score(e)] as const).filter(([, s]) => s > 0).sort((a, b) => b[1] - a[1]).map(([e]) => e).slice(0, 40);
  }, [entries, q]);

  useEffect(() => { setCursor(0); }, [q]);
  if (!open || !kb) return null;

  const run = (e: Entry) => { setOpen(false); e.run(); };
  return (
    <div className="fixed inset-0 z-50 bg-black/30 flex items-start justify-center pt-[12vh]" onMouseDown={() => setOpen(false)}>
      <div className="w-[560px] rounded-md border border-border bg-bg-elevated shadow-lg overflow-hidden" onMouseDown={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 px-3 h-11 border-b border-border">
          <Search size={14} className="text-fg-muted" />
          <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search documents, threads, reviews, actions…"
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") { e.preventDefault(); setCursor((c) => Math.min(c + 1, filtered.length - 1)); }
              if (e.key === "ArrowUp") { e.preventDefault(); setCursor((c) => Math.max(c - 1, 0)); }
              if (e.key === "Enter" && filtered[cursor]) run(filtered[cursor]);
            }}
            className="flex-1 bg-transparent outline-none text-sm" />
          <kbd className="text-[10px] text-fg-muted font-mono">esc</kbd>
        </div>
        <div className="max-h-[50vh] overflow-y-auto py-1">
          {filtered.length === 0 && <div className="px-3 py-4 text-xs text-fg-muted">Nothing matches.</div>}
          {filtered.map((e, i) => {
            const showGroup = i === 0 || filtered[i - 1].group !== e.group;
            return (
              <div key={e.id}>
                {showGroup && <div className="px-3 pt-2 pb-1 text-[10px] uppercase tracking-wide text-fg-muted">{e.group}</div>}
                <button onMouseEnter={() => setCursor(i)} onClick={() => run(e)}
                  className={cn("w-full flex items-center gap-2 px-3 h-8 text-left text-sm", i === cursor ? "bg-bg-muted" : "")}>
                  <e.icon size={13} className="text-fg-muted shrink-0" />
                  <span className="truncate">{e.label}</span>
                  {e.hint && <span className="ml-auto text-[11px] text-fg-muted font-mono truncate max-w-[45%]">{e.hint}</span>}
                </button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
