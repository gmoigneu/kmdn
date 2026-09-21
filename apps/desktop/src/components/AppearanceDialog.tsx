// Appearance settings (#77): theme, fonts, text size, notification toggles. Per device.
import { useEffect, useState } from "react";
import { Check, RotateCcw, X } from "lucide-react";
import { MONO_FONTS, TEXT_SIZE, THEMES, UI_FONTS, useAppearance, type Theme, type ThemeChoice } from "@/lib/appearance";
import { getNotifyPrefs, setNotifyPref, type NotifyEvent } from "@/lib/notify";
import { cn } from "@/lib/utils";

function Swatch({ theme }: { theme: Theme | null }) {
  const light = THEMES.find((t) => t.id === "light")!;
  const dark = THEMES.find((t) => t.id === "dark")!;
  const half = (t: Theme, extra?: string) => (
    <div className={cn("flex-1 p-1.5 flex flex-col gap-1", extra)} style={{ background: t.vars.bg }}>
      <div className="h-1.5 w-8 rounded-sm" style={{ background: t.vars.fg }} />
      <div className="h-1.5 w-12 rounded-sm" style={{ background: t.vars["fg-muted"] }} />
      <div className="mt-auto flex gap-1"><span className="size-2 rounded-full" style={{ background: t.vars.accent }} /><span className="size-2 rounded-full" style={{ background: t.vars.ok }} /><span className="size-2 rounded-full" style={{ background: t.vars.warn }} /></div>
    </div>
  );
  return (
    <div className="h-14 w-full rounded-md overflow-hidden border border-border flex">
      {theme ? half(theme) : <>{half(light)}{half(dark)}</>}
    </div>
  );
}

function FontPicker({ label, value, presets, allowSame, onChange }: { label: string; value: string; presets: Record<string, string>; allowSame?: boolean; onChange: (v: string) => void }) {
  const presetName = Object.entries(presets).find(([, v]) => v === value)?.[0];
  const mode = value === "" && allowSame ? "same" : presetName ?? "custom";
  const [custom, setCustom] = useState(mode === "custom" ? value : "");
  return (
    <label className="grid grid-cols-[9rem_1fr] items-center gap-3 text-xs">
      <span className="text-fg-muted">{label}</span>
      <span className="flex gap-2">
        <select value={mode} onChange={(e) => { const m = e.target.value; if (m === "same") onChange(""); else if (m === "custom") onChange(custom || value || presets[Object.keys(presets)[0]]); else onChange(presets[m]); }}
          className="h-7 px-2 rounded-md border border-border bg-bg text-fg">
          {allowSame && <option value="same">Same as interface</option>}
          {Object.keys(presets).map((k) => <option key={k} value={k}>{k}</option>)}
          <option value="custom">Custom…</option>
        </select>
        {mode === "custom" && (
          <input value={custom || value} onChange={(e) => { setCustom(e.target.value); onChange(e.target.value); }} placeholder='"My Font", sans-serif'
            className="flex-1 h-7 px-2 rounded-md border border-border bg-bg font-mono text-[11px]" />
        )}
      </span>
    </label>
  );
}

const NOTIFY: [NotifyEvent, string][] = [
  ["agent_approval", "An agent needs my approval"],
  ["agent_done", "An agent finished while the window was unfocused"],
  ["review_requested", "A new review appeared"],
];

export function AppearanceDialog() {
  const { appearance: a, open, setOpen, update, reset } = useAppearance();
  const [notify, setNotify] = useState(getNotifyPrefs);
  useEffect(() => {
    if (!open) return;
    setNotify(getNotifyPrefs());
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);
  if (!open) return null;
  const choices: { id: ThemeChoice; label: string; theme: Theme | null }[] = [{ id: "system", label: "System", theme: null }, ...THEMES.map((t) => ({ id: t.id as ThemeChoice, label: t.label, theme: t }))];

  return (
    <div className="fixed inset-0 z-50 bg-black/30 flex items-start justify-center pt-[8vh]" onMouseDown={() => setOpen(false)}>
      <div className="w-[640px] max-h-[84vh] overflow-y-auto rounded-md border border-border bg-bg-elevated shadow-lg" onMouseDown={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 px-4 h-11 border-b border-border">
          <h2 className="font-medium">Appearance</h2>
          <span className="text-xs text-fg-muted">Saved on this device</span>
          <span className="ml-auto" />
          <button onClick={reset} className="h-7 px-2 rounded-md border border-border text-xs flex items-center gap-1" title="Back to the defaults"><RotateCcw size={12} /> Reset</button>
          <button onClick={() => setOpen(false)} className="p-1.5 rounded-md hover:bg-bg-muted text-fg-muted" title="Close (Esc)"><X size={14} /></button>
        </div>

        <section className="px-4 pt-4">
          <h3 className="text-[11px] uppercase tracking-wide text-fg-muted">Theme</h3>
          <div className="mt-2 grid grid-cols-5 gap-2">
            {choices.map((c) => (
              <button key={c.id} onClick={() => update({ theme: c.id })}
                className={cn("text-left rounded-md p-1 border", a.theme === c.id ? "border-accent" : "border-transparent hover:border-border")}>
                <Swatch theme={c.theme} />
                <div className="mt-1 flex items-center gap-1 text-[11px] leading-tight"><span className="truncate">{c.label}</span>{a.theme === c.id && <Check size={11} className="ml-auto text-accent shrink-0" />}</div>
              </button>
            ))}
          </div>
          <p className="mt-1 text-[11px] text-fg-muted">System follows the operating system's light or dark setting.</p>
        </section>

        <section className="px-4 pt-4 space-y-2">
          <h3 className="text-[11px] uppercase tracking-wide text-fg-muted">Fonts</h3>
          <FontPicker label="Interface" value={a.uiFont} presets={UI_FONTS} onChange={(v) => update({ uiFont: v })} />
          <FontPicker label="Reading and editing" value={a.proseFont} presets={UI_FONTS} allowSame onChange={(v) => update({ proseFont: v })} />
          <FontPicker label="Code" value={a.monoFont} presets={MONO_FONTS} onChange={(v) => update({ monoFont: v })} />
        </section>

        <section className="px-4 pt-4">
          <h3 className="text-[11px] uppercase tracking-wide text-fg-muted">Text size</h3>
          <div className="mt-2 flex items-center gap-3 text-xs">
            <input type="range" min={TEXT_SIZE.min} max={TEXT_SIZE.max} step={1} value={a.textSize} onChange={(e) => update({ textSize: Number(e.target.value) })} className="w-48 accent-accent" />
            <span className="tabular-nums w-10">{a.textSize}px</span>
            <span className="text-fg-muted">for reading and editing. The interface stays at 13px.</span>
          </div>
          <div className="mt-2 rounded-md border border-border p-3 prose-pane">
            <p className="m-0">Deploys to production are manual and happen <strong>twice a day</strong>. See <span className="text-accent underline">Deploying</span> and <code>kmdn-cli check</code>.</p>
          </div>
        </section>

        <section className="px-4 py-4 space-y-1">
          <h3 className="text-[11px] uppercase tracking-wide text-fg-muted">Notifications</h3>
          {NOTIFY.map(([ev, label]) => (
            <label key={ev} className="flex items-center gap-2 text-xs">
              <input type="checkbox" checked={notify[ev]} onChange={(e) => { setNotifyPref(ev, e.target.checked); setNotify(getNotifyPrefs()); }} className="accent-accent" />
              {label}
            </label>
          ))}
        </section>
      </div>
    </div>
  );
}
