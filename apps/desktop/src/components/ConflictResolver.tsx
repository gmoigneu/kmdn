// Block-level conflict resolver (D21, 05-git-and-review.md). Never shows conflict markers.
// Left: main. Right: this thread. Per differing block: keep main, keep thread, keep both.
import { useMemo, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Check } from "lucide-react";
import { api, type ConflictFile } from "@/lib/api";
import { splitBlocks, type Block } from "@/lib/rdiff/blocks";
import { align, type Op } from "@/lib/rdiff/align";
import { cn } from "@/lib/utils";

type Choice = "main" | "thread" | "both";

interface Hunk { key: string; main: Block | null; thread: Block | null; equal: boolean }

function hunks(mainText: string, threadText: string): Hunk[] {
  const ops: Op[] = align(splitBlocks(mainText), splitBlocks(threadText));
  return ops.map((op, i) => {
    switch (op.type) {
      case "equal": return { key: `${i}`, main: op.a, thread: op.b, equal: true };
      case "modified": return { key: `${i}`, main: op.a, thread: op.b, equal: false };
      case "removed": return { key: `${i}`, main: op.a, thread: null, equal: false };
      case "added": return { key: `${i}`, main: null, thread: op.b, equal: false };
    }
  });
}

function assemble(hs: Hunk[], choices: Record<string, Choice>): string {
  const parts: string[] = [];
  for (const h of hs) {
    if (h.equal) { parts.push(h.main!.text); continue; }
    const c = choices[h.key] ?? "thread";
    if (c === "main" && h.main) parts.push(h.main.text);
    else if (c === "thread" && h.thread) parts.push(h.thread.text);
    else if (c === "both") { if (h.main) parts.push(h.main.text); if (h.thread) parts.push(h.thread.text); }
  }
  return parts.join("\n").replace(/\n{3,}/g, "\n\n") + "\n";
}

function FileResolver({ file, onResolved }: { file: ConflictFile; onResolved: (content: string) => void }) {
  const hs = useMemo(() => hunks(file.main ?? "", file.thread ?? ""), [file.main, file.thread]);
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const conflicts = hs.filter((h) => !h.equal);
  const done = conflicts.every((h) => choices[h.key]);
  return (
    <section className="rounded-md border border-border">
      <div className="flex items-center gap-2 px-3 h-8 border-b border-border text-xs font-mono">
        <span className="truncate">{file.path}</span>
        <span className="ml-auto text-fg-muted">{conflicts.length} differing block{conflicts.length === 1 ? "" : "s"}</span>
        <button disabled={!done} onClick={() => onResolved(assemble(hs, choices))} className="h-6 px-2 rounded-md bg-accent text-accent-fg disabled:opacity-40 flex items-center gap-1"><Check size={11} /> Use this</button>
      </div>
      <div className="divide-y divide-border">
        {hs.map((h) => h.equal ? (
          <div key={h.key} className="px-3 py-1 text-xs text-fg-muted whitespace-pre-wrap opacity-60">{h.main!.text}</div>
        ) : (
          <div key={h.key} className="p-3 space-y-2">
            <div className="grid grid-cols-2 gap-3 text-xs">
              <div className={cn("rounded-md border p-2 whitespace-pre-wrap", choices[h.key] === "main" || choices[h.key] === "both" ? "border-accent" : "border-border")}>
                <div className="text-[10px] uppercase tracking-wide text-fg-muted mb-1">On main</div>{h.main?.text ?? <span className="text-fg-muted italic">(removed)</span>}
              </div>
              <div className={cn("rounded-md border p-2 whitespace-pre-wrap", choices[h.key] === "thread" || choices[h.key] === "both" ? "border-accent" : "border-border")}>
                <div className="text-[10px] uppercase tracking-wide text-fg-muted mb-1">This thread</div>{h.thread?.text ?? <span className="text-fg-muted italic">(removed)</span>}
              </div>
            </div>
            <div className="flex gap-2 text-xs">
              {(["main", "thread", "both"] as Choice[]).map((c) => (
                <button key={c} onClick={() => setChoices((p) => ({ ...p, [h.key]: c }))}
                  className={cn("h-6 px-2 rounded-md border", choices[h.key] === c ? "bg-bg-muted border-accent" : "border-border")}>
                  {c === "main" ? "Keep main" : c === "thread" ? "Keep mine" : "Keep both"}
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

export function ConflictResolver({ root, slug, files, onDone, onCancel }: { root: string; slug: string; files: ConflictFile[]; onDone: () => void; onCancel: () => void }) {
  const [resolved, setResolved] = useState<Record<string, string>>({});
  const remaining = files.filter((f) => !(f.path in resolved));
  const apply = useMutation({
    mutationFn: () => api.resolveThreadConflicts(root, slug, resolved),
    onSuccess: (out) => { if (typeof out === "object" && "Conflicts" in out) { setResolved({}); } else onDone(); },
  });
  return (
    <div className="flex-1 overflow-y-auto">
      <div className="max-w-[1000px] mx-auto p-6 space-y-4">
        <div className="flex items-center gap-3">
          <h2 className="font-medium">This thread conflicts with main</h2>
          <span className="text-xs text-fg-muted">Pick what to keep for each differing block. Nothing is written until you apply.</span>
          <span className="ml-auto" />
          <button onClick={onCancel} className="h-7 px-3 rounded-md border border-border text-xs">Later</button>
          <button disabled={remaining.length > 0 || apply.isPending} onClick={() => apply.mutate()} className="h-7 px-3 rounded-md bg-accent text-accent-fg text-xs disabled:opacity-40">
            {apply.isPending ? "Applying…" : `Apply ${Object.keys(resolved).length}/${files.length}`}
          </button>
        </div>
        {apply.error && <p className="text-xs text-danger">{String(apply.error)}</p>}
        {files.map((f) => (f.path in resolved
          ? <div key={f.path} className="rounded-md border border-ok/50 px-3 h-8 flex items-center gap-2 text-xs font-mono"><Check size={12} className="text-ok" /> {f.path} <button onClick={() => setResolved((r) => { const n = { ...r }; delete n[f.path]; return n; })} className="ml-auto text-fg-muted">change</button></div>
          : <FileResolver key={f.path} file={f} onResolved={(content) => setResolved((r) => ({ ...r, [f.path]: content }))} />
        ))}
      </div>
    </div>
  );
}
