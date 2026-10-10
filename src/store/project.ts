import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { create } from "zustand";

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

const RECENT_KEY = "recentProjects";
const MAX_RECENT = 8;

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
    const picked = await open({ directory: true, title: "프로젝트 폴더 선택" });
    if (typeof picked === "string") await get().select(picked);
  },

  select: async (path) => {
    const recent = [path, ...get().recent.filter((p) => p !== path)].slice(0, MAX_RECENT);
    saveRecent(recent);
    set({ current: path, recent });
    await get().refresh();
  },

  refresh: async () => {
    const path = get().current;
    if (!path || !isTauri()) return;
    set({ loading: true, error: null });
    try {
      const map = await invoke<ConfigMap>("claude_config_scan", { path });
      // Ignore a late result for a project the user already switched away from.
      if (get().current === path) set({ map, loading: false });
    } catch (e) {
      if (get().current === path) set({ map: null, loading: false, error: String(e) });
    }
  },
}));

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
