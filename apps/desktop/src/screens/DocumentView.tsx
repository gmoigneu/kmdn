import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil } from "lucide-react";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";
import { RenderedMarkdown } from "@/components/RenderedMarkdown";

/** Documents view (D19, D47): read on main; Edit creates a thread named after the document. */
export function DocumentView({ path }: { path: string }) {
  const { kb, go } = useUi();
  const root = kb!.root;
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["doc", root, path], queryFn: () => api.readDocument(root, path) });
  const docs = useQuery({ queryKey: ["docs", root], queryFn: () => api.listDocuments(root) });
  const title = docs.data?.find((d) => d.path === path)?.title ?? path;
  const edit = useMutation({
    mutationFn: () => api.createThread(root, title),
    onSuccess: (t) => { qc.invalidateQueries({ queryKey: ["threads", root] }); go({ kind: "thread", slug: t.slug, openPath: path }); },
  });
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <header className="h-11 shrink-0 border-b border-border flex items-center gap-3 px-4">
        <h1 className="font-medium truncate">{title}</h1>
        <span className="font-mono text-[11px] text-fg-muted truncate">{path}</span>
        <span className="ml-auto" />
        <button onClick={() => edit.mutate()} disabled={edit.isPending} className="h-7 px-3 rounded-md border border-border text-xs flex items-center gap-1 disabled:opacity-40" title="Start a thread to edit this document">
          <Pencil size={12} /> {edit.isPending ? "Starting…" : "Edit"}
        </button>
      </header>
      {edit.error && <p className="px-4 py-1 text-xs text-danger border-b border-border">{String(edit.error)}</p>}
      <div className="flex-1 overflow-y-auto p-8">
        {q.data != null && <RenderedMarkdown text={q.data} />}
      </div>
    </div>
  );
}
