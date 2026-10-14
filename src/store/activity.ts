import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { ToolId } from "../tools";

// Mirrors src-tauri/src/activity/mod.rs.
export type GitEventKind =
  | "commit"
  | "push"
  | "merge"
  | "branch"
  | "prOpened"
  | "prMerged"
  | "prClosed"
  | "ci";

export interface GitEvent {
  id: string;
  repo: string;
  kind: GitEventKind;
  /** Epoch milliseconds. */
  at: number;
  branch: string | null;
  sha: string | null;
  parents: string[];
  pr: number | null;
  title: string | null;
  state: string | null;
  url: string | null;
  author: string | null;
}

export interface BranchTip {
  name: string;
  local: string | null;
  remote: string | null;
  isDefault: boolean;
  isCurrent: boolean;
}

export interface PromptEvent {
  id: string;
  repo: string;
  tool: ToolId;
  session: string;
  at: number;
  branch: string | null;
  text: string;
  files: string[];
  /** Commit shas made during the prompt; may be short. */
  commits: string[];
  pushed: boolean;
}

export interface SessionInfo {
  tool: ToolId;
  id: string;
  started: number;
  ended: number;
  branch: string | null;
  prompts: number;
}

export interface GitLog {
  repo: string;
  events: GitEvent[];
  branches: BranchTip[];
}

export interface PromptLog {
  repo: string;
  events: PromptEvent[];
  sessions: SessionInfo[];
}

export interface ActivitySettings {
  /** `null` keeps everything. */
  retentionDays: number | null;
  syncPrompts: boolean;
}

export type TreeView = "git" | "prompts";
type LogKind = "git" | "prompts";

const PAGE = 200;
const VIEW_KEY = "activity.view";

function loadView(): TreeView {
  try {
    return localStorage.getItem(VIEW_KEY) === "prompts" ? "prompts" : "git";
  } catch {
    return "git";
  }
}

interface ActivityState {
  project: string | null;
  git: GitLog | null;
  prompts: PromptLog | null;
  /** Older entries may exist beyond what is loaded. */
  moreGit: boolean;
  morePrompts: boolean;
  loading: boolean;
  refreshing: boolean;
  loadingMore: LogKind | null;
  error: string | null;
  view: TreeView;
  /** A commit (sha or prefix) the GitHub tree should scroll to and highlight. */
  focusSha: string | null;
  settings: ActivitySettings | null;
  settingsError: string | null;

  setView: (view: TreeView) => void;
  /** Switches to the GitHub tree and highlights the commit. */
  showCommit: (sha: string) => void;
  clearFocus: () => void;
  load: (project: string | null) => Promise<void>;
  /** Re-reads the sources (git, and GitHub when signed in), then the logs. */
  refresh: (github: boolean) => Promise<void>;
  reload: () => Promise<void>;
  loadMore: (kind: LogKind) => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSettings: (patch: Partial<ActivitySettings>) => Promise<void>;
}

export const useActivityStore = create<ActivityState>((set, get) => {
  let seq = 0;

  /** Fetches the newest `limit` entries of both logs. */
  const fetchLogs = async (project: string, limit: number) => {
    const [git, prompts] = await Promise.all([
      invoke<GitLog>("activity_git_log", { project, limit, before: null }),
      invoke<PromptLog>("activity_prompt_log", { project, limit, before: null }),
    ]);
    return { git, prompts };
  };

  return {
    project: null,
    git: null,
    prompts: null,
    moreGit: false,
    morePrompts: false,
    loading: false,
    refreshing: false,
    loadingMore: null,
    error: null,
    view: loadView(),
    focusSha: null,
    settings: null,
    settingsError: null,

    setView: (view) => {
      try {
        localStorage.setItem(VIEW_KEY, view);
      } catch {
        // Only a convenience.
      }
      set({ view });
    },
    showCommit: (sha) => {
      get().setView("git");
      set({ focusSha: sha });
    },
    clearFocus: () => set({ focusSha: null }),

    load: async (project) => {
      if (get().project !== project) {
        seq++;
        set({
          project,
          git: null,
          prompts: null,
          moreGit: false,
          morePrompts: false,
          error: null,
          focusSha: null,
        });
      }
      if (!project) return;
      set({ loading: true });
      await get().reload();
      set({ loading: false });
    },

    reload: async () => {
      const { project, git, prompts } = get();
      if (!project || !isTauri()) return;
      const id = ++seq;
      // Keep the older pages the user already opened.
      const limit = Math.max(PAGE, git?.events.length ?? 0, prompts?.events.length ?? 0);
      try {
        const next = await fetchLogs(project, limit);
        if (id !== seq || get().project !== project) return;
        set({
          git: next.git,
          prompts: next.prompts,
          moreGit: next.git.events.length >= limit,
          morePrompts: next.prompts.events.length >= limit,
          error: null,
        });
      } catch (e) {
        if (id === seq) set({ error: String(e) });
      }
    },

    refresh: async (github) => {
      const { project } = get();
      if (!project || !isTauri() || get().refreshing) return;
      set({ refreshing: true });
      try {
        await invoke("activity_refresh", { project, github });
      } catch (e) {
        // Keep showing what is stored; report it when nothing is.
        if (get().project === project && !get().git) set({ error: String(e) });
      } finally {
        set({ refreshing: false });
      }
      if (get().project === project) await get().reload();
    },

    loadMore: async (kind) => {
      const { project, git, prompts, loadingMore } = get();
      if (!project || !isTauri() || loadingMore) return;
      const events = kind === "git" ? git?.events : prompts?.events;
      if (!events?.length) return;
      const before = Math.min(...events.map((e) => e.at));
      set({ loadingMore: kind });
      try {
        if (kind === "git") {
          const page = await invoke<GitLog>("activity_git_log", { project, limit: PAGE, before });
          if (get().project !== project) return;
          const seen = new Set(events.map((e) => e.id));
          const fresh = page.events.filter((e) => !seen.has(e.id));
          set((s) => ({
            git: s.git && {
              ...s.git,
              events: [...s.git.events, ...fresh],
              branches: page.branches,
            },
            moreGit: fresh.length > 0 && page.events.length >= PAGE,
          }));
        } else {
          const page = await invoke<PromptLog>("activity_prompt_log", {
            project,
            limit: PAGE,
            before,
          });
          if (get().project !== project) return;
          const seen = new Set(events.map((e) => e.id));
          const fresh = page.events.filter((e) => !seen.has(e.id));
          set((s) => ({
            prompts: s.prompts && {
              ...s.prompts,
              events: [...s.prompts.events, ...fresh],
              sessions: page.sessions,
            },
            morePrompts: fresh.length > 0 && page.events.length >= PAGE,
          }));
        }
      } catch (e) {
        set({ error: String(e) });
      } finally {
        set({ loadingMore: null });
      }
    },

    loadSettings: async () => {
      if (!isTauri()) return;
      try {
        set({ settings: await invoke<ActivitySettings>("activity_settings"), settingsError: null });
      } catch (e) {
        set({ settingsError: String(e) });
      }
    },

    saveSettings: async (patch) => {
      const current = get().settings;
      if (!current) return;
      const settings = { ...current, ...patch };
      set({ settings, settingsError: null });
      try {
        await invoke("activity_set_settings", { settings });
      } catch (e) {
        set({ settings: current, settingsError: String(e) });
      }
    },
  };
});

let listening = false;

/** Reloads the logs when the backend says they changed (either window). */
export function listenActivityChanges() {
  if (listening || !isTauri()) return;
  listening = true;
  void listen("activity-changed", () => {
    void useActivityStore.getState().reload();
  });
}
