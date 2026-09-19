import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";

export function DocumentView({ path }: { path: string }) {
  const { kb } = useUi();
  const root = kb!.root;
  const q = useQuery({ queryKey: ["doc", root, path], queryFn: () => api.readDocument(root, path) });
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <header className="h-11 shrink-0 border-b border-border flex items-center gap-3 px-4">
        <h1 className="font-mono text-xs truncate">{path}</h1>
        <button disabled className="ml-auto h-7 px-3 rounded-md border border-border text-xs disabled:opacity-40" title="Edit lands in #30">Edit</button>
      </header>
      <div className="flex-1 overflow-y-auto p-8">
        <pre className="prose-pane whitespace-pre-wrap font-mono text-[13px]">{q.data}</pre>
      </div>
    </div>
  );
}
