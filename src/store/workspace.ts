import { create } from "zustand";

export type Page =
  | "home"
  | "composer"
  | "library"
  | "config"
  | "files"
  | "mcp"
  | "work"
  | "memo"
  | "settings";

const COLLAPSED_KEY = "nav-collapsed";

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

interface WorkspaceState {
  page: Page;
  /** The left navigation shows icons only. */
  collapsed: boolean;
  go: (page: Page) => void;
  toggleCollapsed: () => void;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  page: "home",
  collapsed: readCollapsed(),
  go: (page) => set({ page }),
  toggleCollapsed: () => {
    const collapsed = !get().collapsed;
    try {
      localStorage.setItem(COLLAPSED_KEY, collapsed ? "1" : "0");
    } catch {
      // Only a convenience.
    }
    set({ collapsed });
  },
}));
