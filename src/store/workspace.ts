import { create } from "zustand";
import { getPref, setPref } from "./prefs";

export type Page =
  | "home"
  | "composer"
  | "library"
  | "config"
  | "files"
  | "mcp"
  | "commands"
  | "work"
  | "trees"
  | "memo"
  | "automations"
  | "settings";

const COLLAPSED_KEY = "nav-collapsed";

function readCollapsed(): boolean {
  try {
    return getPref(COLLAPSED_KEY) === "1";
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
    setPref(COLLAPSED_KEY, collapsed ? "1" : "0");
    set({ collapsed });
  },
}));
