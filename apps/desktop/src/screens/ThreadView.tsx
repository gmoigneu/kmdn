import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Bot, Check, ExternalLink, FilePlus, MoreHorizontal, RotateCcw, Save, Trash2 } from "lucide-react";
import { api, type AgentMode, type FileChange, type SubmitOutcome } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";
import { RenderedDiff } from "@/components/RenderedDiff";
import { RenderedMarkdown } from "@/components/RenderedMarkdown";
import { MarkdownEditor } from "@/components/MarkdownEditor";
import { AgentPanel } from "@/components/AgentPanel";
import { ConflictResolver } from "@/components/ConflictResolver";

type Tab = "changes" | "editor" | "read";

const statusLabel: Record<FileChange["status"], string> = { added: "A", modified: "M", deleted: "D", renamed: "R" };

export function ThreadView({ slug, initialPath, initialMode }: { slug: string; initialPath?: string; initialMode?: AgentMode }) {
  const { kb, go, conflicts, setConflicts } = useUi();
  const root = kb!.root;
  const qc = useQueryClient();
  const [resolving, setResolving] = useState(false);
  const conflictFiles = conflicts[slug];
  const threads = useQuery({ queryKey: ["threads", root], queryFn: () => api.listThreads(root) });
  const t = threads.data?.find((x) => x.slug === slug);
  const reviews = useQuery({ queryKey: ["reviews", root], queryFn: () => api.listReviews(root), enabled: kb!.authenticated, retry: false });
  const pr = reviews.data?.find((p) => p.head_branch === t?.branch);
  const [tab, setTab] = useState<Tab>(initialPath ? "editor" : "changes");
  const [openPath, setOpenPath] = useState<string | null>(initialPath ?? null);
  const [menu, setMenu] = useState(false);
  const [buffer, setBuffer] = useState("");
  const [dirty, setDirty] = useState(false);
  const [submitOpen, setSubmitOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [agentLog, setAgentLog] = useState("");
  const [postLog, setPostLog] = useState(true);
  const preview = useQuery({ queryKey: ["submit-preview", root, slug], queryFn: () => api.submitPreview(root, slug), enabled: submitOpen });
  useEffect(() => {
    if (preview.data) { setAgentLog(preview.data.agent_log ?? ""); setPostLog(preview.data.post_agent_log && !!preview.data.agent_log); }
  }, [preview.data]);
  const [outcome, setOutcome] = useState<SubmitOutcome | null>(null);

  const changes = useQuery({ queryKey: ["changes", root, slug], queryFn: () => api.threadChanges(root, slug), enabled: !!t });
  const pending = useQuery({ queryKey: ["pending", root, slug], queryFn: () => api.agentPending(root, slug), enabled: !!t });
  const pendingSet = new Set(pending.data ?? []);
  const refreshChanges = () => { qc.invalidateQueries({ queryKey: ["changes", root, slug] }); qc.invalidateQueries({ queryKey: ["pending", root, slug] }); qc.invalidateQueries({ queryKey: ["wt-docs", t?.path] }); qc.invalidateQueries({ queryKey: ["wt-file", t?.path] }); };
  const accept = useMutation({ mutationFn: (paths: string[]) => api.agentAccept(root, slug, paths), onSuccess: refreshChanges });
  const revert = useMutation({ mutationFn: (paths: string[]) => api.agentRevert(root, slug, paths), onSuccess: refreshChanges });
  const docs = useQuery({ queryKey: ["wt-docs", t?.path], queryFn: () => api.listDocuments(t!.path), enabled: !!t });
  const file = useQuery({ queryKey: ["wt-file", t?.path, openPath], queryFn: () => api.readDocument(t!.path, openPath!), enabled: !!t && !!openPath });
  useEffect(() => { if (file.data != null && !dirty) setBuffer(file.data); }, [file.data, dirty]);

  const save = useMutation({
    mutationFn: () => api.saveDocument(root, slug, openPath!, buffer),
    onSuccess: () => {
      setDirty(false);
      qc.invalidateQueries({ queryKey: ["changes", root, slug] });
      qc.invalidateQueries({ queryKey: ["wt-docs", t?.path] });
      qc.invalidateQueries({ queryKey: ["wt-file", t?.path, openPath] });
    },
  });
  const abandon = useMutation({
    mutationFn: () => api.abandonThread(root, slug),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["threads", root] }); go({ kind: "home" }); },
  });
  const submit = useMutation({
    mutationFn: () => api.submitThread(root, slug, title.trim(), summary.trim() || null, postLog && agentLog.trim() ? agentLog : null),
    onSuccess: (o) => { setOutcome(o); if (o.submission) { setSubmitOpen(false); qc.invalidateQueries({ queryKey: ["reviews", root] }); qc.invalidateQueries({ queryKey: ["changes", root, slug] }); } },
    onError: (e) => setOutcome({ submission: null, findings: [], error: String(e) }),
  });

  function openInEditor(path: string) { setOpenPath(path); setDirty(false); setTab("editor"); }
  function newDocument() {
    const name = window.prompt("New document path (relative, .md)", "untitled.md");
    if (!name) return;
    const p = name.endsWith(".md") ? name : `${name}.md`;
    const stem = p.replace(/\.md$/, "").split("/").pop();
    setOpenPath(p);
    setBuffer(`---\ntitle: ${stem}\nstatus: draft\n---\n# ${stem}\n\n`);
    setDirty(true);
    setTab("editor");
  }

  if (!t) return <div className="p-6 text-fg-muted">Loading thread…</div>;
  const n = changes.data?.length ?? 0;
  const status = pr ? (pr.draft ? "draft" : "in review") : "draft";

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <header className="h-11 shrink-0 border-b border-border flex items-center gap-3 px-4">
        <h1 className="font-medium truncate">{pr?.title ?? t.slug}</h1>
        <span className={cn("text-[10px] px-1.5 rounded-full border border-border", pr ? "text-accent border-accent" : "text-fg-muted")}>{status}</span>
        {pr && <button onClick={() => openUrl(pr.url)} className="text-xs text-fg-muted flex items-center gap-1 hover:text-fg"><ExternalLink size={11} /> #{pr.number}</button>}
        <span className="ml-auto" />
        {pr ? (
          <button onClick={() => go({ kind: "review", number: pr.number })} className="h-7 px-3 rounded-md bg-accent text-accent-fg text-xs">View review</button>
        ) : (
          <button disabled={n === 0 || !kb!.authenticated || pendingSet.size > 0} onClick={() => { setTitle(""); setOutcome(null); setSubmitOpen(true); }}
            className="h-7 px-3 rounded-md bg-accent text-accent-fg text-xs disabled:opacity-40" title={!kb!.authenticated ? "Sign in first" : pendingSet.size > 0 ? "Accept or revert the agent's edits first" : ""}>
            Submit for review
          </button>
        )}
        <div className="relative">
          <button onClick={() => setMenu((m) => !m)} className="p-1.5 rounded-md hover:bg-bg-muted text-fg-muted" title="More"><MoreHorizontal size={14} /></button>
          {menu && (
            <div className="absolute right-0 top-8 z-10 w-48 rounded-md border border-border bg-bg-elevated shadow-sm py-1 text-xs">
              <div className="px-3 py-1 text-fg-muted font-mono truncate" title={t.branch}>{t.branch}</div>
              <button onClick={() => { setMenu(false); if (window.confirm(pr ? "Abandon this thread? The open review stays on the provider; the local worktree and branch are removed." : "Abandon this thread and delete its changes?")) abandon.mutate(); }}
                className="w-full text-left px-3 py-1.5 hover:bg-bg-muted text-danger flex items-center gap-2"><Trash2 size={12} /> Abandon thread</button>
            </div>
          )}
        </div>
      </header>

      {submitOpen && (
        <div className="border-b border-border bg-bg-muted px-4 py-3 space-y-2">
          <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder={preview.data ? `Title (empty uses "${preview.data.default_title}")` : "Title (leave empty for an automatic one)"} className="w-full h-8 px-2 rounded-md border border-border bg-bg text-sm" />
          <textarea value={summary} onChange={(e) => setSummary(e.target.value)} rows={2} placeholder="Summary for reviewers (optional)" className="w-full rounded-md border border-border bg-bg p-2 text-sm resize-none" />
          {preview.data?.agent_log && (
            <div className="rounded-md border border-border bg-bg p-2 space-y-1">
              <label className="flex items-center gap-2 text-xs">
                <input type="checkbox" checked={postLog} onChange={(e) => setPostLog(e.target.checked)} />
                Post how this change was made as a comment on the review. Edit it below or untick to keep it private.
              </label>
              <textarea value={agentLog} onChange={(e) => setAgentLog(e.target.value)} rows={Math.min(8, Math.max(3, agentLog.split("\n").length))} disabled={!postLog}
                className="w-full rounded-md border border-border bg-bg-muted p-2 text-xs font-mono resize-none disabled:opacity-50" />
            </div>
          )}
          <div className="flex items-center gap-2">
            <button onClick={() => submit.mutate()} disabled={submit.isPending} className="h-7 px-3 rounded-md bg-accent text-accent-fg text-xs disabled:opacity-40">{submit.isPending ? "Submitting…" : pr ? "Update review" : "Open review"}</button>
            <button onClick={() => setSubmitOpen(false)} className="h-7 px-3 rounded-md border border-border text-xs">Cancel</button>
            <span className="text-xs text-fg-muted">Runs checks, updates the index, pushes, and opens a pull request.</span>
          </div>
          {outcome?.error && (
            <div className="text-xs text-danger">
              {outcome.error}
              {outcome.findings.length > 0 && <ul className="mt-1 font-mono">{outcome.findings.map((f, i) => <li key={i}>{f.level} {f.path}: {f.message}</li>)}</ul>}
            </div>
          )}
        </div>
      )}
      {conflictFiles && !resolving && (
        <div className="border-b border-warn/50 bg-bg-muted px-4 py-2 text-xs flex items-center gap-2">
          <span className="text-warn">This thread conflicts with main in {conflictFiles.length} file{conflictFiles.length === 1 ? "" : "s"}.</span>
          <button onClick={() => setResolving(true)} className="h-6 px-2 rounded-md bg-accent text-accent-fg">Resolve</button>
        </div>
      )}
      {outcome?.submission && !submitOpen && (
        <div className="border-b border-border bg-bg-muted px-4 py-2 text-xs flex items-center gap-2">
          <span className="text-ok">{outcome.submission.created ? "Review opened" : "Review updated"}.</span>
          <button onClick={() => openUrl(outcome.submission!.pull.url)} className="underline">{outcome.submission.pull.url}</button>
          <button onClick={() => setOutcome(null)} className="ml-auto text-fg-muted">Dismiss</button>
        </div>
      )}

      {resolving && conflictFiles ? (
        <ConflictResolver root={root} slug={slug} files={conflictFiles}
          onDone={() => { setResolving(false); setConflicts(slug, null); qc.invalidateQueries({ queryKey: ["changes", root, slug] }); qc.invalidateQueries({ queryKey: ["wt-docs", t.path] }); }}
          onCancel={() => setResolving(false)} />
      ) : (
      <div className="flex-1 flex min-h-0">
        <section className="w-[38%] min-w-[320px] border-r border-border flex flex-col">
          <AgentPanel root={root} slug={slug} branch={t.branch} initialMode={initialMode} onChanged={() => { qc.invalidateQueries({ queryKey: ["changes", root, slug] }); qc.invalidateQueries({ queryKey: ["pending", root, slug] }); qc.invalidateQueries({ queryKey: ["wt-docs", t.path] }); qc.invalidateQueries({ queryKey: ["wt-file", t.path] }); }} />
        </section>
        <section className="flex-1 flex flex-col min-w-0">
          <div className="h-9 border-b border-border flex items-center px-2 gap-1 text-xs">
            {(["changes", "editor", "read"] as Tab[]).map((x) => (
              <button key={x} onClick={() => setTab(x)} className={cn("px-2 h-7 rounded-md capitalize", tab === x ? "bg-bg-muted" : "text-fg-muted")}>
                {x}{x === "changes" && n > 0 && <span className="ml-1 tabular-nums text-fg-muted">{n}</span>}
              </button>
            ))}
            <span className="ml-auto" />
            {tab === "editor" && openPath && (
              <button onClick={() => save.mutate()} disabled={!dirty || save.isPending} className="h-7 px-2 rounded-md border border-border flex items-center gap-1 disabled:opacity-40"><Save size={12} /> Save</button>
            )}
            <button onClick={newDocument} className="h-7 px-2 rounded-md border border-border flex items-center gap-1" title="New document"><FilePlus size={12} /> New</button>
          </div>
          <div className="flex-1 overflow-y-auto">
            {tab === "changes" && (
              <div className="p-4 space-y-6">
                {n === 0 && <p className="text-fg-muted text-xs">No changes yet. Open a document in the Editor tab and save.</p>}
                {pendingSet.size > 0 && (
                  <div className="rounded-md border border-warn/50 bg-bg-muted px-3 py-2 text-xs flex items-center gap-2">
                    <Bot size={12} className="text-warn" />
                    <span>{pendingSet.size} agent edit{pendingSet.size === 1 ? "" : "s"} waiting for your review. Accepted edits are committed under the agent's name; reverted ones are restored.</span>
                    <span className="ml-auto" />
                    <button onClick={() => accept.mutate([...pendingSet])} disabled={accept.isPending} className="h-6 px-2 rounded-md bg-accent text-accent-fg flex items-center gap-1 disabled:opacity-40"><Check size={11} /> Accept all</button>
                    <button onClick={() => { if (window.confirm("Revert every pending agent edit?")) revert.mutate([...pendingSet]); }} disabled={revert.isPending} className="h-6 px-2 rounded-md border border-border flex items-center gap-1 disabled:opacity-40"><RotateCcw size={11} /> Revert all</button>
                  </div>
                )}
                {changes.data?.map((c) => (
                  <div key={c.path} className={cn("rounded-md border", pendingSet.has(c.path) ? "border-warn/60" : "border-border")}>
                    <div className="flex items-center gap-2 px-3 h-8 border-b border-border text-xs font-mono">
                      <button onClick={() => openInEditor(c.path)} className="flex items-center gap-2 min-w-0 hover:underline text-left">
                        <span className={cn("w-4 text-center", c.status === "added" ? "text-ok" : c.status === "deleted" ? "text-danger" : "text-warn")}>{statusLabel[c.status]}</span>
                        <span className="truncate">{c.path}</span>
                      </button>
                      {pendingSet.has(c.path) && (
                        <>
                          <span className="text-[10px] px-1.5 rounded-full border border-warn/60 text-warn font-sans">agent edit</span>
                          <span className="ml-auto" />
                          <button onClick={() => accept.mutate([c.path])} disabled={accept.isPending} className="h-6 px-2 rounded-md bg-accent text-accent-fg font-sans flex items-center gap-1 disabled:opacity-40"><Check size={11} /> Accept</button>
                          <button onClick={() => revert.mutate([c.path])} disabled={revert.isPending} className="h-6 px-2 rounded-md border border-border font-sans flex items-center gap-1 disabled:opacity-40"><RotateCcw size={11} /> Revert</button>
                        </>
                      )}
                    </div>
                    <div className="py-2"><RenderedDiff change={c} /></div>
                  </div>
                ))}
              </div>
            )}
            {tab === "editor" && (openPath ? (
              <div className="h-full flex flex-col">
                <div className="px-3 h-7 flex items-center text-[11px] font-mono text-fg-muted border-b border-border">{openPath}{dirty && " •"}</div>
                <MarkdownEditor key={openPath} value={buffer} onChange={(next) => { setBuffer(next); setDirty(true); }} onSave={() => { if (dirty) save.mutate(); }} className="flex-1 min-h-0 overflow-hidden" />
                {save.error && <p className="px-3 py-1 text-danger text-xs">{String(save.error)}</p>}
              </div>
            ) : (
              <ul className="p-4 text-xs space-y-1">
                <li className="text-fg-muted mb-2">Open a document to edit it.</li>
                {docs.data?.map((d) => <li key={d.path}><button className="font-mono hover:underline" onClick={() => openInEditor(d.path)}>{d.path}</button></li>)}
              </ul>
            ))}
            {tab === "read" && (
              <div className="p-8 prose-pane">
                {openPath && file.data != null ? <RenderedMarkdown text={buffer || file.data} /> : <p className="text-fg-muted text-xs">Pick a document in the Editor tab.</p>}
              </div>
            )}
          </div>
        </section>
      </div>
      )}
    </div>
  );
}
