// Appearance preferences (#77): theme, fonts, text size. Stored per device in localStorage (D54:
// viewer conveniences), applied as CSS custom properties on <html> so every pane, the sidebar,
// the diff colours and the editor follow along.
import { create } from "zustand";

export const TOKENS = [
  "bg", "bg-muted", "bg-elevated", "fg", "fg-muted", "border", "accent", "accent-fg", "ok", "warn", "danger",
  "syn-keyword", "syn-string", "syn-comment", "syn-number", "syn-function", "syn-type", "syn-property", "syn-operator",
] as const;
export type Token = (typeof TOKENS)[number];

export type ThemeId =
  | "light" | "dark"
  | "catppuccin-latte" | "catppuccin-frappe" | "catppuccin-macchiato" | "catppuccin-mocha"
  | "nord" | "gruvbox-dark" | "gruvbox-light";
export type ThemeChoice = ThemeId | "system";

export interface Theme { id: ThemeId; label: string; scheme: "light" | "dark"; vars: Record<Token, string> }

/** Catppuccin flavours share one structure: https://github.com/catppuccin/catppuccin (MIT). */
function catppuccin(id: ThemeId, label: string, scheme: "light" | "dark", p: Record<string, string>): Theme {
  return {
    id, label, scheme,
    vars: {
      bg: p.base, "bg-muted": p.mantle, "bg-elevated": scheme === "dark" ? p.surface0 : p.base,
      fg: p.text, "fg-muted": p.subtext0, border: scheme === "dark" ? p.surface1 : p.surface0,
      accent: p.blue, "accent-fg": scheme === "dark" ? p.crust : p.base, ok: p.green, warn: p.peach, danger: p.red,
      "syn-keyword": p.mauve, "syn-string": p.green, "syn-comment": p.overlay0, "syn-number": p.peach,
      "syn-function": p.blue, "syn-type": p.yellow, "syn-property": p.lavender, "syn-operator": p.sky,
    },
  };
}

export const THEMES: Theme[] = [
  {
    id: "light", label: "Light", scheme: "light",
    vars: {
      bg: "#ffffff", "bg-muted": "#f6f7f9", "bg-elevated": "#ffffff", fg: "#111318", "fg-muted": "#5c6370", border: "#e3e6ea",
      accent: "#4f46e5", "accent-fg": "#ffffff", ok: "#15803d", warn: "#b45309", danger: "#b91c1c",
      "syn-keyword": "#7c3aed", "syn-string": "#15803d", "syn-comment": "#6b7280", "syn-number": "#b45309",
      "syn-function": "#1d4ed8", "syn-type": "#0f766e", "syn-property": "#4f46e5", "syn-operator": "#374151",
    },
  },
  {
    id: "dark", label: "Dark", scheme: "dark",
    vars: {
      bg: "#0f1115", "bg-muted": "#151820", "bg-elevated": "#1a1e27", fg: "#e6e8ec", "fg-muted": "#8b93a1", border: "#262b36",
      accent: "#818cf8", "accent-fg": "#0f1115", ok: "#4ade80", warn: "#fbbf24", danger: "#f87171",
      "syn-keyword": "#c4b5fd", "syn-string": "#86efac", "syn-comment": "#6b7280", "syn-number": "#fdba74",
      "syn-function": "#93c5fd", "syn-type": "#5eead4", "syn-property": "#a5b4fc", "syn-operator": "#cbd5e1",
    },
  },
  catppuccin("catppuccin-latte", "Catppuccin Latte", "light", {
    base: "#eff1f5", mantle: "#e6e9ef", crust: "#dce0e8", surface0: "#ccd0da", surface1: "#bcc0cc", overlay0: "#9ca0b0",
    text: "#4c4f69", subtext0: "#6c6f85", blue: "#1e66f5", lavender: "#7287fd", mauve: "#8839ef", green: "#40a02b",
    yellow: "#df8e1d", peach: "#fe640b", red: "#d20f39", sky: "#04a5e5",
  }),
  catppuccin("catppuccin-frappe", "Catppuccin Frappé", "dark", {
    base: "#303446", mantle: "#292c3c", crust: "#232634", surface0: "#414559", surface1: "#51576d", overlay0: "#737994",
    text: "#c6d0f5", subtext0: "#a5adce", blue: "#8caaee", lavender: "#babbf1", mauve: "#ca9ee6", green: "#a6d189",
    yellow: "#e5c890", peach: "#ef9f76", red: "#e78284", sky: "#99d1db",
  }),
  catppuccin("catppuccin-macchiato", "Catppuccin Macchiato", "dark", {
    base: "#24273a", mantle: "#1e2030", crust: "#181926", surface0: "#363a4f", surface1: "#494d64", overlay0: "#6e738d",
    text: "#cad3f5", subtext0: "#a5adcb", blue: "#8aadf4", lavender: "#b7bdf8", mauve: "#c6a0f6", green: "#a6da95",
    yellow: "#eed49f", peach: "#f5a97f", red: "#ed8796", sky: "#91d7e3",
  }),
  catppuccin("catppuccin-mocha", "Catppuccin Mocha", "dark", {
    base: "#1e1e2e", mantle: "#181825", crust: "#11111b", surface0: "#313244", surface1: "#45475a", overlay0: "#6c7086",
    text: "#cdd6f4", subtext0: "#a6adc8", blue: "#89b4fa", lavender: "#b4befe", mauve: "#cba6f7", green: "#a6e3a1",
    yellow: "#f9e2af", peach: "#fab387", red: "#f38ba8", sky: "#89dceb",
  }),
  {
    // Nord (MIT): polar night for surfaces, snow storm for text, frost for the accent, aurora for
    // status colours. The muted text is a blend between nord3 and nord4 for legibility.
    id: "nord", label: "Nord", scheme: "dark",
    vars: {
      bg: "#2e3440", "bg-muted": "#3b4252", "bg-elevated": "#3b4252", fg: "#eceff4", "fg-muted": "#a0aabb", border: "#434c5e",
      accent: "#88c0d0", "accent-fg": "#2e3440", ok: "#a3be8c", warn: "#d08770", danger: "#bf616a",
      "syn-keyword": "#81a1c1", "syn-string": "#a3be8c", "syn-comment": "#616e88", "syn-number": "#b48ead",
      "syn-function": "#88c0d0", "syn-type": "#8fbcbb", "syn-property": "#d8dee9", "syn-operator": "#81a1c1",
    },
  },
  {
    id: "gruvbox-dark", label: "Gruvbox Dark", scheme: "dark",
    vars: {
      bg: "#282828", "bg-muted": "#1d2021", "bg-elevated": "#3c3836", fg: "#ebdbb2", "fg-muted": "#a89984", border: "#504945",
      accent: "#fe8019", "accent-fg": "#282828", ok: "#b8bb26", warn: "#fabd2f", danger: "#fb4934",
      "syn-keyword": "#fb4934", "syn-string": "#b8bb26", "syn-comment": "#928374", "syn-number": "#d3869b",
      "syn-function": "#8ec07c", "syn-type": "#fabd2f", "syn-property": "#83a598", "syn-operator": "#ebdbb2",
    },
  },
  {
    id: "gruvbox-light", label: "Gruvbox Light", scheme: "light",
    vars: {
      bg: "#fbf1c7", "bg-muted": "#f2e5bc", "bg-elevated": "#fbf1c7", fg: "#3c3836", "fg-muted": "#7c6f64", border: "#d5c4a1",
      accent: "#af3a03", "accent-fg": "#fbf1c7", ok: "#79740e", warn: "#b57614", danger: "#9d0006",
      "syn-keyword": "#9d0006", "syn-string": "#79740e", "syn-comment": "#928374", "syn-number": "#8f3f71",
      "syn-function": "#427b58", "syn-type": "#b57614", "syn-property": "#076678", "syn-operator": "#3c3836",
    },
  },
];

export const UI_FONTS: Record<string, string> = {
  Inter: 'Inter, ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
  System: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
  Serif: 'Georgia, "Iowan Old Style", "Palatino Linotype", "Source Serif Pro", serif',
  Humanist: '"Avenir Next", Avenir, "Segoe UI", Candara, "Trebuchet MS", sans-serif',
};
export const MONO_FONTS: Record<string, string> = {
  "JetBrains Mono": '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace',
  System: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
  "Fira Code": '"Fira Code", ui-monospace, SFMono-Regular, Menlo, monospace',
  "IBM Plex Mono": '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, monospace',
};

export interface Appearance {
  theme: ThemeChoice;
  /** CSS font-family lists. `proseFont` empty means "same as the interface font". */
  uiFont: string;
  proseFont: string;
  monoFont: string;
  /** Reading and editing size in px. The interface stays at 13px. */
  textSize: number;
}

export const DEFAULTS: Appearance = { theme: "system", uiFont: UI_FONTS.Inter, proseFont: "", monoFont: MONO_FONTS["JetBrains Mono"], textSize: 15 };
export const TEXT_SIZE = { min: 12, max: 22 };
const KEY = "kmdn.appearance";

/** Accepts anything (old or hand-edited storage) and returns a valid Appearance. */
export function normalize(raw: unknown): Appearance {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const theme = typeof r.theme === "string" && (r.theme === "system" || THEMES.some((t) => t.id === r.theme)) ? (r.theme as ThemeChoice) : DEFAULTS.theme;
  const str = (v: unknown, d: string) => (typeof v === "string" && v.length <= 300 ? v : d);
  const size = typeof r.textSize === "number" && Number.isFinite(r.textSize) ? Math.min(TEXT_SIZE.max, Math.max(TEXT_SIZE.min, Math.round(r.textSize))) : DEFAULTS.textSize;
  return { theme, uiFont: str(r.uiFont, DEFAULTS.uiFont) || DEFAULTS.uiFont, proseFont: str(r.proseFont, ""), monoFont: str(r.monoFont, DEFAULTS.monoFont) || DEFAULTS.monoFont, textSize: size };
}

export function load(): Appearance {
  try { return normalize(JSON.parse(localStorage.getItem(KEY) ?? "null")); } catch { return { ...DEFAULTS }; }
}
function save(a: Appearance) {
  try { localStorage.setItem(KEY, JSON.stringify(a)); } catch { /* storage may be unavailable */ }
}

/** Writes the preferences onto <html>: data-theme, color-scheme, and one custom property per token. */
export function apply(a: Appearance) {
  const root = document.documentElement;
  const theme = THEMES.find((t) => t.id === a.theme);
  if (theme) {
    root.dataset.theme = theme.id;
    root.style.colorScheme = theme.scheme;
    for (const k of TOKENS) root.style.setProperty(`--${k}`, theme.vars[k]);
  } else {
    delete root.dataset.theme;
    root.style.removeProperty("color-scheme");
    for (const k of TOKENS) root.style.removeProperty(`--${k}`);
  }
  root.style.setProperty("--font-ui", a.uiFont);
  root.style.setProperty("--font-prose", a.proseFont || a.uiFont);
  root.style.setProperty("--font-mono", a.monoFont);
  root.style.setProperty("--reading-size", `${a.textSize}px`);
}

interface AppearanceState {
  appearance: Appearance;
  open: boolean;
  setOpen: (open: boolean) => void;
  update: (patch: Partial<Appearance>) => void;
  reset: () => void;
}

export const useAppearance = create<AppearanceState>((set, get) => ({
  appearance: load(),
  open: false,
  setOpen: (open) => set({ open }),
  update: (patch) => { const next = normalize({ ...get().appearance, ...patch }); save(next); apply(next); set({ appearance: next }); },
  reset: () => { const next = { ...DEFAULTS }; save(next); apply(next); set({ appearance: next }); },
}));

/** Call once before the first render so the stored theme paints without a flash. */
export function initAppearance() { apply(load()); }
