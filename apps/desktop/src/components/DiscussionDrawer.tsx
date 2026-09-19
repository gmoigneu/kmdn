// Discussion on a published document (D13): one provider issue per path, shown as a drawer.
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink } from "lucide-react";

import { api } from "@/lib/api";

export function DiscussionDrawer({ root, path }: { root: string; path: string }) {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["discussion", root, path], queryFn: () => api.docDiscussion(root, path), retry: false });
  const [body, setBody] = useState("");
  const post = useMutation({
    mutationFn: () => api.docDiscussionComment(root, path, body.trim()),
    onSuccess: (d) => { setBody(""); qc.setQueryData(["discussion", root, path], d); },
  });
  return (
    <aside className="w-80 shrink-0 border-l border-border flex flex-col">
      <div className="h-9 border-b border-border flex items-center px-3 text-xs text-fg-muted gap-2">
        Discussion
        {q.data?.issue && <button onClick={() => openUrl(q.data!.issue!.url)} className="ml-auto flex items-center gap-1 hover:text-fg"><ExternalLink size={11} /> #{q.data.issue.number}</button>}
      </div>
      <div className="flex-1 overflow-y-auto p-3 space-y-3 text-xs">
        {q.isLoading && <p className="text-fg-muted">Loading…</p>}
        {q.error && <p className="text-danger">{String(q.error)}</p>}
        {q.data && !q.data.issue && <p className="text-fg-muted">No discussion yet. A comment opens one on the provider, tagged kmdn and titled with this document's path.</p>}
        {q.data?.comments.map((c) => (
          <div key={c.id} className="rounded-md border border-border px-3 py-2">
            <div className="flex items-center gap-2"><span className="font-medium">{c.author}</span><span className="text-fg-muted">{c.created_at.slice(0, 10)}</span></div>
            <div className="mt-1 whitespace-pre-wrap break-words">{c.body}</div>
          </div>
        ))}
      </div>
      <div className="border-t border-border p-2 space-y-1">
        <textarea value={body} onChange={(e) => setBody(e.target.value)} rows={3} placeholder="Say something about this document, e.g. this section is outdated" className="w-full resize-none rounded-md border border-border bg-bg p-2 text-sm outline-none" />
        <div className="flex justify-end">
          <button disabled={!body.trim() || post.isPending} onClick={() => post.mutate()} className="h-7 px-3 rounded-md border border-border text-xs disabled:opacity-40">{post.isPending ? "Posting…" : "Comment"}</button>
        </div>
        {post.error && <p className="text-xs text-danger">{String(post.error)}</p>}
      </div>
    </aside>
  );
}
