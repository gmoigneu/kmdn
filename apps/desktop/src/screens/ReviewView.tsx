// Review layout (D48): full-width rendered diff, comments drawer, approve / request changes / merge.
import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, ExternalLink, GitMerge, MessageSquare, PanelRightOpen, X } from "lucide-react";
import { api, type FileChange } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";
import { renderDiff, commentAnchor, groupRows, type RenderedRow } from "@/lib/rdiff/render";
import { UnchangedRun } from "@/components/RenderedDiff";
import { RenderedMarkdown } from "@/components/RenderedMarkdown";

const statusLabel: Record<FileChange["status"], string> = { added: "A", modified: "M", deleted: "D", renamed: "R" };

function FileDiff({ change, onComment }: { change: FileChange; onComment: (path: string, row: RenderedRow) => void }) {
  const groups = useMemo(() => groupRows(renderDiff(change.old ?? "", change.new ?? "")), [change.old, change.new]);
  if (change.binary) return <p className="text-fg-muted text-xs px-3">Binary file, {change.status}.</p>;
  const renderRow = (r: RenderedRow, i: number) => {
    const tone = r.type === "added" ? "border-l-ok" : r.type === "removed" ? "border-l-danger" : r.type === "modified" ? "border-l-warn" : "border-l-transparent";
    return (
      <div key={i} className={cn("group relative px-3 py-1 border-l-2", tone, r.type === "equal" && "opacity-60")}>
        <button onClick={() => onComment(change.path, r)} title="Comment on this block"
          className="absolute -left-7 top-1 size-5 rounded border border-border bg-bg-elevated text-fg-muted opacity-0 group-hover:opacity-100 grid place-items-center">
          <MessageSquare size={11} />
        </button>
        {r.type === "modified" ? (
          <div className="grid grid-cols-2 gap-3"><div className="opacity-70" dangerouslySetInnerHTML={{ __html: r.old ?? "" }} /><div dangerouslySetInnerHTML={{ __html: r.new ?? "" }} /></div>
        ) : (
          <div dangerouslySetInnerHTML={{ __html: r.new ?? r.old ?? "" }} />
        )}
      </div>
    );
  };
  return (
    <div className="rdiff text-[14px]">
      {groups.map((g, i) => g.kind === "row" ? renderRow(g.row, g.index) : <UnchangedRun key={`u${i}`} group={g} render={renderRow} />)}
    </div>
  );
}

export function ReviewView({ number }: { number: number }) {
  const { kb } = useUi();
  const root = kb!.root;
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["review", root, number], queryFn: () => api.reviewDetail(root, number), refetchInterval: 60_000 });
  const [drawer, setDrawer] = useState(true);
  const [body, setBody] = useState("");
  const [anchor, setAnchor] = useState<{ path: string; line: number; side: "left" | "right" } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => { qc.invalidateQueries({ queryKey: ["review", root, number] }); qc.invalidateQueries({ queryKey: ["reviews", root] }); };
  const comment = useMutation({
    mutationFn: () => api.reviewComment(root, number, body.trim(), anchor?.path, anchor?.line, anchor?.side),
    onSuccess: () => { setBody(""); setAnchor(null); refresh(); },
    onError: (e) => setError(String(e)),
  });
  const act = useMutation({
    mutationFn: async (kind: "approve" | "request_changes" | "merge") => {
      setBusy(kind);
      if (kind === "merge") return api.reviewMerge(root, number, "squash");
      return api.reviewSubmit(root, number, kind, body.trim());
    },
    onSuccess: () => { setBody(""); setBusy(null); refresh(); qc.invalidateQueries({ queryKey: ["docs", root] }); },
    onError: (e) => { setBusy(null); setError(String(e)); },
  });

  if (q.isLoading) return <div className="p-6 text-fg-muted text-xs">Loading review…</div>;
  if (q.error || !q.data) return <div className="p-6 text-danger text-xs">{String(q.error ?? "not found")}</div>;
  const { pull, changes, comments, mergeability: m } = q.data;
  const canMerge = m.mergeable !== false && !m.changes_requested && m.checks_passing !== false && pull.state === "open";
  const inline = comments.filter((c) => c.path);
  const general = comments.filter((c) => !c.path);

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <header className="h-11 shrink-0 border-b border-border flex items-center gap-3 px-4">
        <span className="text-fg-muted tabular-nums text-xs">#{pull.number}</span>
        <h1 className="font-medium truncate">{pull.title}</h1>
        <span className={cn("text-[10px] px-1.5 rounded-full border", pull.state === "merged" ? "border-ok text-ok" : "border-border text-fg-muted")}>{pull.state === "merged" ? "published" : pull.draft ? "draft" : "in review"}</span>
        <span className="text-xs text-fg-muted">by {pull.author}</span>
        <span className="text-xs text-fg-muted">·</span>
        <span className="text-xs text-fg-muted" title={m.state}>
          {m.approvals} approval{m.approvals === 1 ? "" : "s"}{m.changes_requested && ", changes requested"}{m.checks_passing === false && ", checks failing"}{m.mergeable === false && ", conflicts"}
        </span>
        <span className="ml-auto" />
        <button onClick={() => openUrl(pull.url)} className="h-7 px-2 rounded-md border border-border text-xs flex items-center gap-1"><ExternalLink size={12} /> Open on provider</button>
        {pull.state === "open" && (
          <>
            <button disabled={!!busy} onClick={() => act.mutate("request_changes")} className="h-7 px-2 rounded-md border border-border text-xs flex items-center gap-1 disabled:opacity-40"><X size={12} /> Request changes</button>
            <button disabled={!!busy} onClick={() => act.mutate("approve")} className="h-7 px-2 rounded-md border border-border text-xs flex items-center gap-1 disabled:opacity-40"><Check size={12} /> Approve</button>
            <button disabled={!!busy || !canMerge} onClick={() => act.mutate("merge")} title={canMerge ? "Squash and merge" : "Not mergeable yet"}
              className="h-7 px-3 rounded-md bg-accent text-accent-fg text-xs flex items-center gap-1 disabled:opacity-40"><GitMerge size={12} /> {busy === "merge" ? "Publishing…" : "Publish"}</button>
          </>
        )}
        <button onClick={() => setDrawer((d) => !d)} title="Conversation" className="p-1.5 rounded-md hover:bg-bg-muted text-fg-muted"><PanelRightOpen size={14} /></button>
      </header>
      {error && <div className="px-4 py-1 text-xs text-danger border-b border-border flex items-center gap-2">{error}<button onClick={() => setError(null)} className="ml-auto">Dismiss</button></div>}

      <div className="flex-1 flex min-h-0">
        <div className="flex-1 overflow-y-auto">
          <div className="max-w-[900px] mx-auto px-10 py-6 space-y-8">
            {pull.body && <div className="rounded-md border border-border p-4 text-sm"><RenderedMarkdown text={pull.body} /></div>}
            {changes.map((c) => (
              <section key={c.path} className="rounded-md border border-border">
                <div className="flex items-center gap-2 px-3 h-8 border-b border-border text-xs font-mono">
                  <span className={cn("w-4 text-center", c.status === "added" ? "text-ok" : c.status === "deleted" ? "text-danger" : "text-warn")}>{statusLabel[c.status]}</span>
                  <span className="truncate">{c.path}</span>
                  {inline.filter((x) => x.path === c.path).length > 0 && <span className="ml-auto text-fg-muted flex items-center gap-1"><MessageSquare size={11} /> {inline.filter((x) => x.path === c.path).length}</span>}
                </div>
                <div className="py-2 pl-8 pr-2">
                  <FileDiff change={c} onComment={(path, row) => { const a = commentAnchor(row); setAnchor({ path, line: a.line, side: a.side === "RIGHT" ? "right" : "left" }); setDrawer(true); }} />
                  {inline.filter((x) => x.path === c.path).map((x) => (
                    <div key={x.id} className="mt-2 mx-3 rounded-md border border-border bg-bg-muted px-3 py-2 text-xs">
                      <span className="font-medium">{x.author}</span> <span className="text-fg-muted">line {x.line}</span>
                      <div className="mt-1 whitespace-pre-wrap">{x.body}</div>
                    </div>
                  ))}
                </div>
              </section>
            ))}
            {changes.length === 0 && <p className="text-xs text-fg-muted">No file changes.</p>}
          </div>
        </div>

        {drawer && (
          <aside className="w-80 shrink-0 border-l border-border flex flex-col">
            <div className="h-9 border-b border-border flex items-center px-3 text-xs text-fg-muted">Conversation · {general.length}</div>
            <div className="flex-1 overflow-y-auto p-3 space-y-3 text-xs">
              {general.map((c) => (
                <div key={c.id} className="rounded-md border border-border px-3 py-2">
                  <div className="flex items-center gap-2"><span className="font-medium">{c.author}</span><span className="text-fg-muted">{c.created_at.slice(0, 10)}</span></div>
                  <div className="mt-1 whitespace-pre-wrap break-words">{c.body.replace(/<!--[\s\S]*?-->\n?/g, "").replace(/<\/?details>|<\/?summary>/g, "")}</div>
                </div>
              ))}
              {general.length === 0 && <p className="text-fg-muted">No comments yet.</p>}
            </div>
            <div className="border-t border-border p-2 space-y-1">
              {anchor && <div className="text-[11px] text-fg-muted flex items-center gap-1 font-mono">on {anchor.path}:{anchor.line}<button onClick={() => setAnchor(null)} className="ml-auto">clear</button></div>}
              <textarea value={body} onChange={(e) => setBody(e.target.value)} rows={3} placeholder={anchor ? "Inline comment…" : "Comment, or a note to go with Approve / Request changes"}
                className="w-full resize-none rounded-md border border-border bg-bg p-2 text-sm outline-none" />
              <div className="flex justify-end">
                <button disabled={!body.trim() || comment.isPending} onClick={() => comment.mutate()} className="h-7 px-3 rounded-md border border-border text-xs disabled:opacity-40">Comment</button>
              </div>
            </div>
          </aside>
        )}
      </div>
    </div>
  );
}
