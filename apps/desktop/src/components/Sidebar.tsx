import { useEffect, useRef } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FileText, GitPullRequest, HardDrive, Layers, Palette, PanelLeft, RefreshCw, Search } from "lucide-react";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";
import { notify } from "@/lib/notify";
import { useAppearance } from "@/lib/appearance";

function Section({ title, icon: Icon, count, children }: { title: string; icon: typeof Layers; count?: number; children: React.ReactNode }) {
  return (
    <section className="px-2 pt-3">
      <div className="flex items-center gap-2 px-2 py-1 text-[11px] uppercase tracking-wide text-fg-muted">
        <Icon size={12} />
        <span className="flex-1">{title}</span>
        {count != null && <span className="tabular-nums">{count}</span>}
      </div>
      <div className="mt-1">{children}</div>
    </section>
  );
}

/** Fetch, fast-forward, rebase (D31): on focus and every 60s, backing off when unfocused. */
function useSyncLoop(root: string) {
  const qc = useQueryClient();
  const setConflicts = useUi((s) => s.setConflicts);
  const sync = useMutation({
    mutationFn: () => api.syncNow(root),
    onSuccess: (report) => {
      for (const [slug, outcome] of report.threads) {
        if ("Ok" in outcome && typeof outcome.Ok === "object" && "Conflicts" in outcome.Ok) setConflicts(slug, outcome.Ok.Conflicts);
        else if ("Ok" in outcome) setConflicts(slug, null);
      }
      // Invalidate only what the report says moved (review P3): a full document scan costs
      // seconds at 5,000 documents, so it runs only when main actually advanced.
      const mainMoved = report.main !== "UpToDate" && !(typeof report.main === "object" && "Skipped" in report.main);
      const rebased = report.threads.filter(([, o]) => "Ok" in o && typeof o.Ok === "object" && "Rebased" in o.Ok).map(([slug]) => slug);
      qc.invalidateQueries({ queryKey: ["threads", root] });
      qc.invalidateQueries({ queryKey: ["local", root] });
      qc.invalidateQueries({ queryKey: ["reviews", root] });
      if (mainMoved) { qc.invalidateQueries({ queryKey: ["docs", root] }); api.reindexKb(root).catch(() => {}); }
      for (const slug of rebased) { qc.invalidateQueries({ queryKey: ["changes", root, slug] }); qc.invalidateQueries({ queryKey: ["wt-docs"] }); }
    },
  });
  useEffect(() => {
    let interval = 60_000;
    let timer: ReturnType<typeof setTimeout>;
    let unfocusedSince: number | null = null;
    const tick = () => {
      if (unfocusedSince && Date.now() - unfocusedSince > 10 * 60_000) interval = 5 * 60_000;
      sync.mutate();
      timer = setTimeout(tick, interval);
    };
    const onFocus = () => { unfocusedSince = null; interval = 60_000; clearTimeout(timer); tick(); };
    const onBlur = () => { unfocusedSince = Date.now(); };
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    timer = setTimeout(tick, 2_000);
    return () => { clearTimeout(timer); window.removeEventListener("focus", onFocus); window.removeEventListener("blur", onBlur); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);
  return sync;
}

export function Sidebar() {
  const { kb, view, go, sidebarCollapsed, toggleSidebar, conflicts } = useUi();
  const openAppearance = useAppearance((s) => s.setOpen);
  const root = kb!.root;
  const qc = useQueryClient();
  const threads = useQuery({ queryKey: ["threads", root], queryFn: () => api.listThreads(root) });
  const docs = useQuery({ queryKey: ["docs", root], queryFn: () => api.listDocuments(root) });
  const local = useQuery({ queryKey: ["local", root], queryFn: () => api.localChanges(root), refetchInterval: 30_000 });
  const reviews = useQuery({ queryKey: ["reviews", root], queryFn: () => api.listReviews(root), enabled: kb!.authenticated, retry: false });
  const seen = useRef<Set<number> | null>(null);
  useEffect(() => {
    if (!reviews.data) return;
    if (seen.current === null) { seen.current = new Set(reviews.data.map((p) => p.number)); return; }
    for (const p of reviews.data) {
      if (!seen.current.has(p.number)) {
        seen.current.add(p.number);
        if (p.author !== kb!.user.name) notify("review_requested", "kmdn: new review", `#${p.number} ${p.title} by ${p.author}`);
      }
    }
  }, [reviews.data, kb]);
  const sync = useSyncLoop(root);
  // Background full-text index (D33): the tree is already on screen when this runs.
  useEffect(() => { api.reindexKb(root).catch(() => {}); }, [root]);
  const move = useMutation({
    mutationFn: () => api.moveLocalChangesToThread(root, "local-changes"),
    onSuccess: (t) => { qc.invalidateQueries({ queryKey: ["threads", root] }); qc.invalidateQueries({ queryKey: ["local", root] }); go({ kind: "thread", slug: t.slug }); },
  });
  const adopt = useMutation({
    mutationFn: (branch: string) => api.adoptBranch(root, branch),
    onSuccess: (t) => { qc.invalidateQueries({ queryKey: ["threads", root] }); qc.invalidateQueries({ queryKey: ["local", root] }); go({ kind: "thread", slug: t.slug }); },
  });
  const hasLocal = local.data && (local.data.dirty_paths.length > 0 || local.data.foreign_branch || local.data.operation_in_progress);

  if (sidebarCollapsed) {
    return (
      <aside className="w-11 border-r border-border bg-bg-muted flex flex-col items-center py-2 gap-3">
        <button title="Expand sidebar" onClick={toggleSidebar} className="p-1.5 rounded-md hover:bg-bg-elevated"><PanelLeft size={16} /></button>
        <button title="Home" onClick={() => go({ kind: "home" })} className="p-1.5 rounded-md hover:bg-bg-elevated"><Layers size={16} /></button>
        <button title="Reviews" onClick={() => { toggleSidebar(); }} className="p-1.5 rounded-md hover:bg-bg-elevated"><GitPullRequest size={16} /></button>
        <button title="Documents" onClick={() => { toggleSidebar(); }} className="p-1.5 rounded-md hover:bg-bg-elevated"><FileText size={16} /></button>
        <button title="Appearance" onClick={() => openAppearance(true)} className="mt-auto p-1.5 rounded-md hover:bg-bg-elevated text-fg-muted"><Palette size={16} /></button>
      </aside>
    );
  }

  return (
    <aside className="w-64 shrink-0 border-r border-border bg-bg-muted flex flex-col overflow-hidden">
      <div className="flex items-center gap-1 px-3 h-11 border-b border-border">
        <button onClick={() => go({ kind: "home" })} className="font-medium truncate flex-1 text-left">
          {kb!.config.name ?? kb!.remote?.name ?? "Knowledge base"}
        </button>
        <button title={sync.isPending ? "Syncing…" : "Sync now"} onClick={() => sync.mutate()} className="p-1 rounded-md hover:bg-bg-elevated text-fg-muted">
          <RefreshCw size={14} className={cn(sync.isPending && "animate-spin")} />
        </button>
        <button title="Appearance" onClick={() => openAppearance(true)} className="p-1 rounded-md hover:bg-bg-elevated text-fg-muted"><Palette size={14} /></button>
        <button title="Collapse sidebar" onClick={toggleSidebar} className="p-1 rounded-md hover:bg-bg-elevated text-fg-muted"><PanelLeft size={14} /></button>
      </div>
      <div className="px-3 py-2">
        <button onClick={() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true }))}
          className="w-full flex items-center gap-2 px-2 h-7 rounded-md border border-border bg-bg text-fg-muted text-xs hover:bg-bg-elevated">
          <Search size={12} /> Search <span className="ml-auto font-mono text-[10px]">⌘K</span>
        </button>
      </div>
      <div className="flex-1 overflow-y-auto pb-4">
        <Section title="Threads" icon={Layers} count={(threads.data?.length ?? 0) + (hasLocal ? 1 : 0)}>
          {hasLocal && (
            <div className="px-2 py-1.5 rounded-md border border-dashed border-border mb-1">
              <div className="flex items-center gap-2 text-xs"><HardDrive size={12} className="text-fg-muted" /><span className="flex-1">Local changes</span><span className="text-[10px] text-fg-muted">read-only</span></div>
              <div className="text-[11px] text-fg-muted mt-0.5">
                {local.data!.foreign_branch ? `on ${local.data!.foreign_branch}` : `${local.data!.dirty_paths.length} file(s) changed in the clone`}
                {local.data!.operation_in_progress && `, ${local.data!.operation_in_progress} in progress`}
              </div>
              {!local.data!.foreign_branch && !local.data!.operation_in_progress && (
                <button onClick={() => move.mutate()} disabled={move.isPending} className="mt-1.5 h-6 px-2 rounded-md border border-border text-[11px] hover:bg-bg-elevated disabled:opacity-40">Move to new thread</button>
              )}
              {local.data!.foreign_branch && !local.data!.operation_in_progress && (
                <button onClick={() => adopt.mutate(local.data!.foreign_branch!)} disabled={adopt.isPending} className="mt-1.5 h-6 px-2 rounded-md border border-border text-[11px] hover:bg-bg-elevated disabled:opacity-40">Adopt branch as thread</button>
              )}
              {(move.error || adopt.error) && <p className="text-danger text-[11px] mt-1">{String(move.error ?? adopt.error)}</p>}
            </div>
          )}
          {threads.data?.filter((t) => !t.merged_at).length ? threads.data.filter((t) => !t.merged_at).map((t) => (
            <button key={t.slug} onClick={() => go({ kind: "thread", slug: t.slug })}
              className={cn("w-full text-left px-2 py-1 rounded-md truncate hover:bg-bg-elevated flex items-center gap-2",
                view.kind === "thread" && view.slug === t.slug && "bg-bg-elevated")}>
              <span className={cn("size-1.5 rounded-full", conflicts[t.slug] ? "bg-warn" : reviews.data?.some((p) => p.head_branch === t.branch) ? "bg-accent" : "bg-fg-muted")} title={conflicts[t.slug] ? "Conflicts with main" : undefined} />
              <span className="truncate">{reviews.data?.find((p) => p.head_branch === t.branch)?.title ?? t.slug}</span>
              <span className={cn("ml-auto text-[10px] px-1.5 rounded-full border", reviews.data?.some((p) => p.head_branch === t.branch) ? "border-accent text-accent" : "border-border text-fg-muted")}>
                {reviews.data?.some((p) => p.head_branch === t.branch) ? "in review" : "draft"}
              </span>
            </button>
          )) : !hasLocal && <div className="px-2 text-xs text-fg-muted">No threads yet.</div>}
          {(threads.data?.some((t) => t.merged_at)) && (
            <details className="mt-1">
              <summary className="px-2 py-1 text-[11px] text-fg-muted cursor-pointer">Done · {threads.data!.filter((t) => t.merged_at).length}</summary>
              {threads.data!.filter((t) => t.merged_at).map((t) => (
                <button key={t.slug} onClick={() => go({ kind: "thread", slug: t.slug })} className="w-full text-left px-2 py-1 rounded-md truncate hover:bg-bg-elevated flex items-center gap-2 opacity-70">
                  <span className="size-1.5 rounded-full bg-ok" /><span className="truncate">{t.slug}</span>
                  <span className="ml-auto text-[10px] px-1.5 rounded-full border border-ok text-ok">done</span>
                </button>
              ))}
            </details>
          )}
        </Section>
        <Section title="Reviews" icon={GitPullRequest} count={reviews.data?.length}>
          {!kb!.authenticated && <div className="px-2 text-xs text-fg-muted">Sign in to see reviews.</div>}
          {reviews.error && <div className="px-2 text-xs text-danger">{String(reviews.error)}</div>}
          {reviews.data?.map((p) => (
            <button key={p.number} onClick={() => go({ kind: "review", number: p.number })}
              className={cn("w-full text-left px-2 py-1 rounded-md hover:bg-bg-elevated flex items-center gap-2",
                view.kind === "review" && view.number === p.number && "bg-bg-elevated")} title={p.url}>
              <span className="text-fg-muted tabular-nums text-[11px]">#{p.number}</span>
              <span className="truncate flex-1">{p.title}</span>
              <span className="text-[10px] text-fg-muted">{p.author}</span>
            </button>
          ))}
          {kb!.authenticated && reviews.data?.length === 0 && <div className="px-2 text-xs text-fg-muted">No open reviews.</div>}
        </Section>
        <Section title="Documents" icon={FileText} count={docs.data?.length}>
          {docs.data?.map((d) => (
            <button key={d.path} onClick={() => go({ kind: "document", path: d.path })}
              className={cn("w-full text-left px-2 py-1 rounded-md truncate hover:bg-bg-elevated",
                view.kind === "document" && view.path === d.path && "bg-bg-elevated")}
              title={d.path}>
              <span className="text-fg-muted">{d.path.includes("/") ? d.path.slice(0, d.path.lastIndexOf("/") + 1) : ""}</span>{d.title}
            </button>
          ))}
        </Section>
      </div>
    </aside>
  );
}
