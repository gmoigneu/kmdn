// First run (D53): sign in, pick or create a repo, choose a folder. Or open an existing clone.
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { FolderOpen, Github, KeyRound, Palette, Plus } from "lucide-react";
import { api, type RepoSummary } from "@/lib/api";
import { useUi } from "@/lib/store";
import { cn } from "@/lib/utils";
import { useAppearance } from "@/lib/appearance";

type Step = "signin" | "pick" | "folder";

export function SignIn() {
  const setKb = useUi((s) => s.setKb);
  const openAppearance = useAppearance((s) => s.setOpen);
  const qc = useQueryClient();
  const auth = useQuery({ queryKey: ["auth"], queryFn: api.authStatus });
  const signedIn = (auth.data?.hosts.length ?? 0) > 0;
  const [step, setStep] = useState<Step>("signin");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (signedIn && step === "signin") setStep("pick"); }, [signedIn, step]);

  // --- sign in
  const [pat, setPat] = useState("");
  const [host, setHost] = useState("github.com");
  const savePat = useMutation({
    mutationFn: () => api.authSavePat(host.trim(), pat.trim()),
    onSuccess: () => { setPat(""); setError(null); qc.invalidateQueries({ queryKey: ["auth"] }); },
    onError: (e) => setError(String(e)),
  });
  const [device, setDevice] = useState<{ user_code: string; verification_uri: string; device_code: string; interval: number } | null>(null);
  const startDevice = useMutation({
    mutationFn: api.authStartDeviceFlow,
    onSuccess: (d) => { setDevice(d); openUrl(d.verification_uri).catch(() => {}); },
    onError: (e) => setError(String(e)),
  });
  useEffect(() => {
    if (!device) return;
    let stop = false;
    const tick = async () => {
      if (stop) return;
      try {
        const r = await api.authPollDeviceFlow(device.device_code);
        if (r === "pending" || r === "slow_down") { setTimeout(tick, (device.interval + (r === "slow_down" ? 5 : 0)) * 1000); return; }
        setDevice(null); qc.invalidateQueries({ queryKey: ["auth"] });
      } catch (e) { setError(String(e)); setDevice(null); }
    };
    const t = setTimeout(tick, device.interval * 1000);
    return () => { stop = true; clearTimeout(t); };
  }, [device, qc]);

  // --- pick
  const primaryHost = auth.data?.hosts[0]?.host ?? "github.com";
  const repos = useQuery({ queryKey: ["remote-repos", primaryHost], queryFn: () => api.listRemoteRepos(primaryHost), enabled: step === "pick" && signedIn });
  const [filter, setFilter] = useState("");
  const [chosen, setChosen] = useState<RepoSummary | null>(null);
  const [url, setUrl] = useState("");
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [newDesc, setNewDesc] = useState("");

  // --- folder
  const [dest, setDest] = useState("");
  useEffect(() => {
    const name = chosen?.name ?? (creating ? newName : url.split("/").pop()?.replace(/\.git$/, "") ?? "");
    if (name) api.defaultCloneDir(name).then(setDest).catch(() => {});
  }, [chosen, url, creating, newName]);

  const openExisting = useMutation({
    mutationFn: async () => { const p = await open({ directory: true }); if (typeof p !== "string") throw new Error("cancelled"); return api.openKb(p); },
    onSuccess: setKb,
    onError: (e) => { if (String(e) !== "Error: cancelled") setError(String(e)); },
  });
  const go = useMutation({
    mutationFn: () => creating ? api.createKb(primaryHost, newName.trim(), newDesc.trim(), null, dest) : api.cloneKb(chosen?.https_url ?? url.trim(), dest),
    onSuccess: setKb,
    onError: (e) => setError(String(e)),
  });

  const list = (repos.data ?? []).filter((r) => r.full_name.toLowerCase().includes(filter.toLowerCase()));

  return (
    <div className="h-full flex items-center justify-center">
      <div className="w-[520px] rounded-md border border-border bg-bg-elevated">
        <div className="flex items-center gap-2 px-5 h-11 border-b border-border text-xs text-fg-muted">
          {(["signin", "pick", "folder"] as Step[]).map((s, i) => (
            <span key={s} className={cn("flex items-center gap-2", step === s && "text-fg")}>
              <span className={cn("size-4 rounded-full grid place-items-center text-[10px] border border-border", step === s && "bg-accent text-accent-fg border-accent")}>{i + 1}</span>
              {s === "signin" ? "Sign in" : s === "pick" ? "Knowledge base" : "Folder"}
              {i < 2 && <span className="w-6 border-t border-border" />}
            </span>
          ))}
          <button onClick={() => openExisting.mutate()} className="ml-auto flex items-center gap-1 hover:text-fg"><FolderOpen size={12} /> Open existing clone</button>
          <button onClick={() => openAppearance(true)} title="Appearance" className="hover:text-fg"><Palette size={12} /></button>
        </div>

        <div className="p-5">
          {step === "signin" && (
            <div className="space-y-4">
              <h1 className="text-lg font-medium">Sign in to GitHub</h1>
              {auth.data?.github_device_flow_available ? (
                device ? (
                  <div className="rounded-md border border-border p-4 text-sm">
                    Enter this code at <button className="text-accent underline" onClick={() => openUrl(device.verification_uri)}>{device.verification_uri}</button>
                    <div className="mt-2 font-mono text-2xl tracking-widest">{device.user_code}</div>
                    <p className="mt-2 text-fg-muted text-xs">Waiting for approval…</p>
                  </div>
                ) : (
                  <button onClick={() => startDevice.mutate()} className="h-9 px-3 rounded-md bg-accent text-accent-fg flex items-center gap-2"><Github size={14} /> Continue with GitHub</button>
                )
              ) : (
                <p className="text-fg-muted text-xs">Device sign-in is not configured in this build. Use a personal access token with <span className="font-mono">repo</span> and <span className="font-mono">read:user</span> scopes.</p>
              )}
              <div className="flex gap-2">
                <input value={host} onChange={(e) => setHost(e.target.value)} placeholder="github.com" title="github.com, gitlab.com, or a self-hosted GitLab host"
                  className="w-44 h-9 px-2 rounded-md border border-border bg-bg font-mono text-xs" />
                <input value={pat} onChange={(e) => setPat(e.target.value)} type="password" placeholder="Personal access token"
                  className="flex-1 h-9 px-2 rounded-md border border-border bg-bg font-mono text-xs" />
                <button onClick={() => savePat.mutate()} disabled={!pat.trim() || !host.trim() || savePat.isPending} className="h-9 px-3 rounded-md border border-border flex items-center gap-2 disabled:opacity-40"><KeyRound size={14} /> {savePat.isPending ? "Checking…" : "Save"}</button>
              </div>
              <p className="text-fg-muted text-xs">GitHub: token with <span className="font-mono">repo</span> and <span className="font-mono">read:user</span>. GitLab or self-hosted GitLab: token with <span className="font-mono">api</span>.</p>
            </div>
          )}

          {step === "pick" && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h1 className="text-lg font-medium">Choose a knowledge base</h1>
                <span className="text-xs text-fg-muted">{auth.data?.hosts.map((h) => `${h.login}@${h.host}`).join(", ")}</span>
              </div>
              {!creating ? (
                <>
                  <input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter repositories" className="w-full h-8 px-2 rounded-md border border-border bg-bg text-xs" />
                  <div className="max-h-56 overflow-y-auto rounded-md border border-border divide-y divide-border">
                    {repos.isLoading && <div className="p-3 text-xs text-fg-muted">Loading…</div>}
                    {repos.error && <div className="p-3 text-xs text-danger">{String(repos.error)}</div>}
                    {list.map((r) => (
                      <button key={r.full_name} onClick={() => { setChosen(r); setUrl(""); }}
                        className={cn("w-full text-left px-3 py-1.5 text-xs hover:bg-bg-muted flex items-center gap-2", chosen?.full_name === r.full_name && "bg-bg-muted")}>
                        <span className="font-mono">{r.full_name}</span>
                        {r.private && <span className="text-[10px] text-fg-muted border border-border rounded px-1">private</span>}
                        <span className="ml-auto truncate text-fg-muted">{r.description}</span>
                      </button>
                    ))}
                  </div>
                  <div className="flex items-center gap-2 text-xs text-fg-muted"><span className="flex-1 border-t border-border" />or<span className="flex-1 border-t border-border" /></div>
                  <input value={url} onChange={(e) => { setUrl(e.target.value); setChosen(null); }} placeholder={`https://${primaryHost}/owner/repo.git`} className="w-full h-8 px-2 rounded-md border border-border bg-bg font-mono text-xs" />
                  <button onClick={() => setCreating(true)} className="text-xs text-accent flex items-center gap-1"><Plus size={12} /> New knowledge base</button>
                </>
              ) : (
                <div className="space-y-2">
                  <input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="repository-name" className="w-full h-8 px-2 rounded-md border border-border bg-bg font-mono text-xs" />
                  <input value={newDesc} onChange={(e) => setNewDesc(e.target.value)} placeholder="What this knowledge base covers" className="w-full h-8 px-2 rounded-md border border-border bg-bg text-xs" />
                  <p className="text-xs text-fg-muted">Created private under your account with a starter template.</p>
                  <button onClick={() => setCreating(false)} className="text-xs text-fg-muted underline">Back to existing repositories</button>
                </div>
              )}
              <div className="flex justify-end">
                <button disabled={!(chosen || url.trim() || (creating && newName.trim()))} onClick={() => setStep("folder")} className="h-8 px-3 rounded-md bg-accent text-accent-fg text-xs disabled:opacity-40">Next</button>
              </div>
            </div>
          )}

          {step === "folder" && (
            <div className="space-y-3">
              <h1 className="text-lg font-medium">Where should it live?</h1>
              <div className="flex gap-2">
                <input value={dest} onChange={(e) => setDest(e.target.value)} className="flex-1 h-8 px-2 rounded-md border border-border bg-bg font-mono text-xs" />
                <button onClick={async () => { const p = await open({ directory: true }); if (typeof p === "string") setDest(p); }} className="h-8 px-3 rounded-md border border-border text-xs">Browse</button>
              </div>
              <p className="text-xs text-fg-muted">Your clone. Other tools can use it too; kmdn keeps it on the default branch and works in separate worktrees next to it.</p>
              <div className="flex justify-between">
                <button onClick={() => setStep("pick")} className="h-8 px-3 rounded-md border border-border text-xs">Back</button>
                <button disabled={!dest || go.isPending} onClick={() => go.mutate()} className="h-8 px-3 rounded-md bg-accent text-accent-fg text-xs disabled:opacity-40">
                  {go.isPending ? (creating ? "Creating…" : "Cloning…") : creating ? "Create" : "Clone and open"}
                </button>
              </div>
            </div>
          )}
          {error && <p className="mt-3 text-danger text-xs break-all">{error}</p>}
        </div>
      </div>
    </div>
  );
}
