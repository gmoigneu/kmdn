import { create } from "zustand";
import type { KbInfo } from "./api";

export type View = { kind: "home" } | { kind: "thread"; slug: string; openPath?: string } | { kind: "document"; path: string } | { kind: "review"; number: number };

interface UiState {
  kb: KbInfo | null;
  view: View;
  sidebarCollapsed: boolean;
  setKb: (kb: KbInfo | null) => void;
  go: (view: View) => void;
  toggleSidebar: () => void;
}

export const useUi = create<UiState>((set) => ({
  kb: null,
  view: { kind: "home" },
  sidebarCollapsed: false,
  setKb: (kb) => set({ kb }),
  go: (view) => set({ view, sidebarCollapsed: view.kind === "thread" }),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
}));
