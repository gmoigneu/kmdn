import { useQuery } from "@tanstack/react-query";
import { FileText, GitPullRequest, Layers, PanelLeft, Search } from "lucide-react";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";

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

export function Sidebar() {
  const { kb, view, go, sidebarCollapsed, toggleSidebar } = useUi();
  const root = kb!.root;
  const threads = useQuery({ queryKey: ["threads", root], queryFn: () => api.listThreads(root) });
  const docs = useQuery({ queryKey: ["docs", root], queryFn: () => api.listDocuments(root) });

  if (sidebarCollapsed) {
    return (
      <aside className="w-11 border-r border-border bg-bg-muted flex flex-col items-center py-2 gap-3">
        <button title="Expand sidebar" onClick={toggleSidebar} className="p-1.5 rounded-md hover:bg-bg-elevated"><PanelLeft size={16} /></button>
        <button title="Home" onClick={() => go({ kind: "home" })} className="p-1.5 rounded-md hover:bg-bg-elevated"><Layers size={16} /></button>
        <button title="Reviews" className="p-1.5 rounded-md hover:bg-bg-elevated"><GitPullRequest size={16} /></button>
        <button title="Documents" className="p-1.5 rounded-md hover:bg-bg-elevated"><FileText size={16} /></button>
      </aside>
    );
  }

  return (
    <aside className="w-64 shrink-0 border-r border-border bg-bg-muted flex flex-col overflow-hidden">
      <div className="flex items-center gap-2 px-3 h-11 border-b border-border">
        <button onClick={() => go({ kind: "home" })} className="font-medium truncate flex-1 text-left">
          {kb!.config.name ?? kb!.remote?.name ?? "Knowledge base"}
        </button>
        <button title="Collapse sidebar" onClick={toggleSidebar} className="p-1 rounded-md hover:bg-bg-elevated text-fg-muted"><PanelLeft size={14} /></button>
      </div>
      <div className="px-3 py-2">
        <div className="flex items-center gap-2 px-2 h-7 rounded-md border border-border bg-bg text-fg-muted text-xs">
          <Search size={12} /> Search <span className="ml-auto font-mono text-[10px]">⌘K</span>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto pb-4">
        <Section title="Threads" icon={Layers} count={threads.data?.length}>
          {threads.data?.length ? threads.data.map((t) => (
            <button key={t.slug} onClick={() => go({ kind: "thread", slug: t.slug })}
              className={cn("w-full text-left px-2 py-1 rounded-md truncate hover:bg-bg-elevated flex items-center gap-2",
                view.kind === "thread" && view.slug === t.slug && "bg-bg-elevated")}>
              <span className="size-1.5 rounded-full bg-fg-muted" />
              <span className="truncate">{t.slug}</span>
              <span className="ml-auto text-[10px] px-1.5 rounded-full border border-border text-fg-muted">draft</span>
            </button>
          )) : <div className="px-2 text-xs text-fg-muted">No threads yet.</div>}
        </Section>
        <Section title="Reviews" icon={GitPullRequest} count={0}>
          <div className="px-2 text-xs text-fg-muted">Connect a provider to see reviews.</div>
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
