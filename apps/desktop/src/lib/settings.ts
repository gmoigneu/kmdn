// Per-viewer app settings that are not appearance. Stored in localStorage (never shared, never
// synced), read through a small store so components re-render when they change.
import { create } from "zustand";

const KEY = "kmdn.settings";

interface Settings {
  /** Shows the Developer agent mode, which allows shell commands with per-call approval (D46). */
  developerMode: boolean;
}

const defaults: Settings = { developerMode: false };

function load(): Settings {
  try { return { ...defaults, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") }; } catch { return defaults; }
}

interface SettingsState extends Settings {
  update: (patch: Partial<Settings>) => void;
}

export const useSettings = create<SettingsState>((set, get) => ({
  ...load(),
  update: (patch) => {
    set(patch);
    const { developerMode } = { ...get(), ...patch };
    try { localStorage.setItem(KEY, JSON.stringify({ developerMode })); } catch { /* storage may be unavailable */ }
  },
}));
