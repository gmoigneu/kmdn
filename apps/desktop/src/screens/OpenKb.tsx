import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/api";
import { useUi } from "@/lib/store";

export function OpenKb() {
  const setKb = useUi((s) => s.setKb);
  const [error, setError] = useState<string | null>(null);
  const [path, setPath] = useState("");

  async function openPath(p: string) {
    try {
      setKb(await api.openKb(p));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="h-full flex items-center justify-center">
      <div className="w-[440px] rounded-md border border-border bg-bg-elevated p-6">
        <h1 className="text-lg font-medium">Open a knowledge base</h1>
        <p className="text-fg-muted mt-1">Sign-in and cloning land in #21. For now, open an existing clone.</p>
        <div className="mt-4 flex gap-2">
          <input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/path/to/clone"
            className="flex-1 h-8 px-2 rounded-md border border-border bg-bg font-mono text-xs" />
          <button onClick={async () => { const p = await open({ directory: true }); if (typeof p === "string") { setPath(p); openPath(p); } }}
            className="h-8 px-3 rounded-md border border-border hover:bg-bg-muted">Browse</button>
        </div>
        <button onClick={() => openPath(path)} disabled={!path}
          className="mt-3 h-8 px-3 rounded-md bg-accent text-accent-fg disabled:opacity-50">Open</button>
        {error && <p className="mt-3 text-danger text-xs">{error}</p>}
      </div>
    </div>
  );
}
