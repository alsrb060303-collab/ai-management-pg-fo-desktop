import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { create } from "zustand";
import { onLocaleApplied, t } from "../i18n";
import { isToolId, type ToolId, type ToolPref } from "../tools";

// Mirrors `ConfigMap` in src-tauri/src/tools/mod.rs.
export type ConfigKind =
  | "instructions"
  | "rule"
  | "skill"
  | "command"
  | "agent"
  | "settings"
  | "mcp"
  | "plugin"
  | "policy"
  | "hook";
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
  tool: ToolId;
  project: string;
  repoRoot: string | null;
  items: ConfigItem[];
  notes: string[];
}

// Mirrors `Detection` in src-tauri/src/tools/detect.rs.
export interface Detection {
  tool: ToolId;
  reason: "files" | "installed" | "fallback";
  signals: { tool: ToolId; path: string }[];
  installed: ToolId[];
}

// Both windows share localStorage; it is the source of truth for the recent
// list and the selected project, since the main window is recreated on every
// switch and the sidebar lives on in the background.
const RECENT_KEY = "recentProjects";
const CURRENT_KEY = "currentProject";
const MAX_RECENT = 8;
/** `{ [project path]: ToolPref }`; a project not listed is "auto". */
const TOOL_PREFS_KEY = "toolPrefs";
const DEFAULT_TOOL_KEY = "defaultTool";
let requestId = 0;

interface ProjectState {
  current: string | null;
  recent: string[];
  map: ConfigMap | null;
  /** The current project's choice and what detection found. */
  toolPref: ToolPref;
  detection: Detection | null;
  /** The tool the app works with for the current project. */
  tool: ToolId;
  /** Used when detection finds nothing to go on. */
  defaultTool: ToolId;
  loading: boolean;
  error: string | null;
  choose: () => Promise<void>;
  select: (path: string) => Promise<void>;
  refresh: () => Promise<void>;
  setToolPref: (pref: ToolPref) => Promise<void>;
  setDefaultTool: (tool: ToolId) => Promise<void>;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  current: null,
  recent: loadRecent(),
  map: null,
  toolPref: "auto",
  detection: null,
  tool: loadDefaultTool(),
  defaultTool: loadDefaultTool(),
  loading: false,
  error: null,

  choose: async () => {
    if (!isTauri()) return;
    const picked = await open({ directory: true, title: t("docs.project.chooseTitle") });
    if (typeof picked === "string") await get().select(picked);
  },

  select: async (path) => {
    if (get().current !== path) {
      set({ current: path, map: null, detection: null, error: null, toolPref: loadToolPref(path) });
    }
    saveCurrent(path);
    await get().refresh();
  },

  refresh: async () => {
    const path = get().current;
    if (!path || !isTauri()) return;
    // A newer request (or a project switch) supersedes this one.
    const id = ++requestId;
    const toolPref = loadToolPref(path);
    const defaultTool = loadDefaultTool();
    set({ loading: true, error: null, toolPref, defaultTool });
    try {
      const detection = await invoke<Detection>("tool_detect", { path, fallback: defaultTool });
      if (id !== requestId) return;
      const tool = toolPref === "auto" ? detection.tool : toolPref;
      set({ detection, tool });
      const map = await invoke<ConfigMap>("config_scan", { path, tool });
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

  setToolPref: async (pref) => {
    const path = get().current;
    if (!path) return;
    saveToolPref(path, pref);
    set({ toolPref: pref, map: null });
    await get().refresh();
  },

  setDefaultTool: async (tool) => {
    try {
      localStorage.setItem(DEFAULT_TOOL_KEY, tool);
    } catch {
      // Only a convenience.
    }
    set({ defaultTool: tool });
    await get().refresh();
  },
}));

/**
 * The backend's "folder not found" error from `config_scan` / `tool_detect`. It is
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

let restoring = false;

/** Whether the current project change is `restoreProject`, not the user's pick. */
export const isRestoringProject = () => restoring;

/** Opens the project selected last (in either window). */
export function restoreProject() {
  const saved = loadCurrent();
  if (saved && saved !== useProjectStore.getState().current) {
    // `select` sets `current` before its first await, so subscribers see the flag.
    restoring = true;
    try {
      void useProjectStore.getState().select(saved);
    } finally {
      restoring = false;
    }
  }
}

window.addEventListener("storage", (event) => {
  if (event.key === RECENT_KEY) useProjectStore.setState({ recent: loadRecent() });
  // A tool change in the other window: follow it.
  if (event.key === TOOL_PREFS_KEY || event.key === DEFAULT_TOOL_KEY) {
    void useProjectStore.getState().refresh();
  }
});

function loadToolPrefs(): Record<string, ToolPref> {
  try {
    const value = JSON.parse(localStorage.getItem(TOOL_PREFS_KEY) ?? "{}");
    return value && typeof value === "object" ? value : {};
  } catch {
    return {};
  }
}

function loadToolPref(path: string): ToolPref {
  const pref = loadToolPrefs()[path];
  return isToolId(pref) ? pref : "auto";
}

function saveToolPref(path: string, pref: ToolPref) {
  const prefs = loadToolPrefs();
  if (pref === "auto") delete prefs[path];
  else prefs[path] = pref;
  try {
    localStorage.setItem(TOOL_PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // The choice just won't persist.
  }
}

function loadDefaultTool(): ToolId {
  try {
    const value = localStorage.getItem(DEFAULT_TOOL_KEY);
    if (isToolId(value)) return value;
  } catch {
    // Fall through to the default.
  }
  return "claude";
}

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
