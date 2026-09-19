import { create } from "zustand";
import type { ConflictFile, KbInfo } from "./api";

export type View = { kind: "home" } | { kind: "thread"; slug: string; openPath?: string } | { kind: "document"; path: string } | { kind: "review"; number: number };

interface UiState {
  kb: KbInfo | null;
  view: View;
  sidebarCollapsed: boolean;
  /** Threads whose last rebase onto main stopped on conflicts (D21). */
  conflicts: Record<string, ConflictFile[]>;
  setConflicts: (slug: string, files: ConflictFile[] | null) => void;
  setKb: (kb: KbInfo | null) => void;
  go: (view: View) => void;
  toggleSidebar: () => void;
}

export const useUi = create<UiState>((set) => ({
  kb: null,
  view: { kind: "home" },
  sidebarCollapsed: false,
  conflicts: {},
  setConflicts: (slug, files) => set((s) => { const c = { ...s.conflicts }; if (files && files.length) c[slug] = files; else delete c[slug]; return { conflicts: c }; }),
  setKb: (kb) => set({ kb, conflicts: {} }),
  go: (view) => set({ view, sidebarCollapsed: view.kind === "thread" }),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
}));
