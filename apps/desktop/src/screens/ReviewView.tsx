// Review layout placeholder (#28): PR header and description until the diff and drawer land.
import { useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink } from "lucide-react";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";
import { RenderedMarkdown } from "@/components/RenderedMarkdown";

export function ReviewView({ number }: { number: number }) {
  const { kb } = useUi();
  const root = kb!.root;
  const reviews = useQuery({ queryKey: ["reviews", root], queryFn: () => api.listReviews(root) });
  const pr = reviews.data?.find((p) => p.number === number);
  if (!pr) return <div className="p-6 text-fg-muted text-xs">Loading review…</div>;
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <header className="h-11 shrink-0 border-b border-border flex items-center gap-3 px-4">
        <span className="text-fg-muted tabular-nums text-xs">#{pr.number}</span>
        <h1 className="font-medium truncate">{pr.title}</h1>
        <span className="text-[10px] px-1.5 rounded-full border border-border text-fg-muted">{pr.draft ? "draft" : "in review"}</span>
        <span className="text-xs text-fg-muted">by {pr.author}</span>
        <span className="ml-auto" />
        <button onClick={() => openUrl(pr.url)} className="h-7 px-2 rounded-md border border-border text-xs flex items-center gap-1"><ExternalLink size={12} /> Open on provider</button>
      </header>
      <div className="flex-1 overflow-y-auto p-8">
        <RenderedMarkdown text={pr.body || "_No description._"} />
        <h2 className="mt-8 text-[11px] uppercase tracking-wide text-fg-muted">Files</h2>
        <ul className="mt-2 text-xs font-mono space-y-1">{pr.files.map((f) => <li key={f}>{f}</li>)}</ul>
        <p className="mt-8 text-xs text-fg-muted">Rendered diff, inline comments, approve and merge land in #28.</p>
      </div>
    </div>
  );
}
