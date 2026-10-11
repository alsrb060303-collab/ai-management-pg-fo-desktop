import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { create } from "zustand";
import { onLocaleApplied, t } from "../i18n";

// Mirrors `ConfigMap` in src-tauri/src/claude_config/mod.rs.
export type ConfigKind =
  | "instructions"
  | "rule"
  | "skill"
  | "command"
  | "agent"
  | "settings"
  | "mcp"
  | "plugin";
export type ConfigScope = "managed" | "user" | "parent" | "project" | "local";
export type ConfigState = "active" | "conditional" | "limited" | "off" | "shadowed" | "notLoaded";

export interface ConfigItem {
  kind: ConfigKind;
  scope: ConfigScope;
  name: string;
  path: string;
  description: string | null;
  state: ConfigState;
  detail: string | null;
}

export interface ConfigMap {
  project: string;
  repoRoot: string | null;
  items: ConfigItem[];
  notes: string[];
}

// Both windows share localStorage; it is the source of truth for the recent
// list and the selected project, since the main window is recreated on every
// switch and the sidebar lives on in the background.
const RECENT_KEY = "recentProjects";
const CURRENT_KEY = "currentProject";
const MAX_RECENT = 8;
let requestId = 0;

interface ProjectState {
  current: string | null;
  recent: string[];
  map: ConfigMap | null;
  loading: boolean;
  error: string | null;
  choose: () => Promise<void>;
  select: (path: string) => Promise<void>;
  refresh: () => Promise<void>;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  current: null,
  recent: loadRecent(),
  map: null,
  loading: false,
  error: null,

  choose: async () => {
    if (!isTauri()) return;
    const picked = await open({ directory: true, title: t("docs.project.chooseTitle") });
    if (typeof picked === "string") await get().select(picked);
  },

  select: async (path) => {
    if (get().current !== path) set({ current: path, map: null, error: null });
    saveCurrent(path);
    await get().refresh();
  },

  refresh: async () => {
    const path = get().current;
    if (!path || !isTauri()) return;
    // A newer request (or a project switch) supersedes this one.
    const id = ++requestId;
    set({ loading: true, error: null });
    try {
      const map = await invoke<ConfigMap>("claude_config_scan", { path });
      if (id !== requestId) return;
      const recent = [path, ...loadRecent().filter((p) => p !== path)].slice(0, MAX_RECENT);
      saveRecent(recent);
      set({ map, recent, loading: false });
    } catch (e) {
      if (id !== requestId) return;
      const error = String(e);
      // Forget folders that are gone, but keep network paths that may be offline.
      const recent =
        isFolderNotFound(error) && !/^[\\/]{2}/.test(path)
          ? loadRecent().filter((p) => p !== path)
          : loadRecent();
      saveRecent(recent);
      set({ map: null, loading: false, error, recent });
    }
  },
}));

/**
 * The backend's "folder not found" error from `claude_config_scan`. It is
 * translated to the app language (Korean or English), so accept both prefixes.
 */
function isFolderNotFound(error: string): boolean {
  return error.startsWith("폴더를 찾을 수 없습니다") || error.startsWith("Folder not found");
}

function saveCurrent(path: string) {
  try {
    localStorage.setItem(CURRENT_KEY, path);
  } catch {
    // Only a convenience.
  }
}

function loadCurrent(): string | null {
  try {
    return localStorage.getItem(CURRENT_KEY);
  } catch {
    return null;
  }
}

/** Opens the project selected last (in either window). */
export function restoreProject() {
  const saved = loadCurrent();
  if (saved && saved !== useProjectStore.getState().current) {
    void useProjectStore.getState().select(saved);
  }
}

window.addEventListener("storage", (event) => {
  if (event.key === RECENT_KEY) useProjectStore.setState({ recent: loadRecent() });
});

function loadRecent(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((v) => typeof v === "string") : [];
  } catch {
    return [];
  }
}

function saveRecent(recent: string[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
  } catch {
    // Storage can be unavailable; the list just won't persist.
  }
}

// Settings-map notes and errors come from the backend: rescan in the new language.
onLocaleApplied(() => void useProjectStore.getState().refresh());
