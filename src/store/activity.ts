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
/** GitHub is polled at most this often, unless the user asks. */
const GITHUB_MIN_MS = 5 * 60_000;

/** The oldest entry of a newest-first log, as the (at, id) paging cursor. */
function oldest(events: { at: number; id: string }[]) {
  let o = events[0];
  for (const e of events) if (e.at < o.at || (e.at === o.at && e.id < o.id)) o = e;
  return { before: o.at, beforeId: o.id };
}

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
  /** A soft problem from the last refresh (e.g. GitHub CI runs couldn't be read). */
  warning: string | null;
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
  refresh: (github: boolean, force?: boolean) => Promise<void>;
  reload: () => Promise<void>;
  /** Resolves true when the page added entries. */
  loadMore: (kind: LogKind) => Promise<boolean>;
  loadSettings: () => Promise<void>;
  saveSettings: (patch: Partial<ActivitySettings>) => Promise<void>;
}

export const useActivityStore = create<ActivityState>((set, get) => {
  let seq = 0;
  /** Refreshes in flight, by project (value: whether GitHub is included). */
  const inflight = new Map<string, boolean>();
  const lastGithub = new Map<string, number>();
  const pending = new Map<string, Promise<void>>();

  /** Fetches the newest `limit` entries of both logs. */
  const fetchLogs = async (project: string, limit: number) => {
    const [git, prompts] = await Promise.all([
      invoke<GitLog>("activity_git_log", { project, limit, before: null, beforeId: null }),
      invoke<PromptLog>("activity_prompt_log", {
        project,
        limit,
        before: null,
        beforeId: null,
      }),
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
    warning: null,
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
          loadingMore: null,
          refreshing: inflight.has(project ?? ""),
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

    refresh: async (github, force = false) => {
      const { project } = get();
      if (!project || !isTauri()) return;
      const wantGithub =
        github && (force || Date.now() - (lastGithub.get(project) ?? 0) >= GITHUB_MIN_MS);
      const running = inflight.get(project);
      if (running !== undefined) {
        // Same project already refreshing: wait, then add GitHub if it lacked it.
        await pending.get(project);
        if (wantGithub && !running) await get().refresh(true, true);
        return;
      }
      inflight.set(project, wantGithub);
      if (wantGithub) lastGithub.set(project, Date.now());
      const work = (async () => {
        set({ refreshing: true });
        try {
          const res = await invoke<{ warning?: string | null }>("activity_refresh", {
            project,
            github: wantGithub,
          });
          // Only a GitHub refresh can report or clear a GitHub warning.
          if (wantGithub && get().project === project) set({ warning: res?.warning ?? null });
        } catch (e) {
          // Keep showing what is stored; report it when nothing is.
          if (get().project === project && !get().git) set({ error: String(e) });
        } finally {
          inflight.delete(project);
          pending.delete(project);
          if (get().project === project) set({ refreshing: false });
        }
        if (get().project === project) await get().reload();
      })();
      pending.set(project, work);
      await work;
    },

    loadMore: async (kind) => {
      const { project, git, prompts, loadingMore } = get();
      if (!project || !isTauri() || loadingMore) return false;
      const events = kind === "git" ? git?.events : prompts?.events;
      if (!events?.length) return false;
      const cursor = oldest(events);
      set({ loadingMore: kind });
      try {
        const seen = new Set(events.map((e) => e.id));
        if (kind === "git") {
          const page = await invoke<GitLog>("activity_git_log", {
            project,
            limit: PAGE,
            ...cursor,
          });
          if (get().project !== project) return false;
          const fresh = page.events.filter((e) => !seen.has(e.id));
          set((s) => ({
            git: s.git && {
              ...s.git,
              events: [...s.git.events, ...fresh],
              branches: page.branches,
            },
            moreGit: fresh.length > 0 && page.events.length >= PAGE,
          }));
          return fresh.length > 0;
        }
        const page = await invoke<PromptLog>("activity_prompt_log", {
          project,
          limit: PAGE,
          ...cursor,
        });
        if (get().project !== project) return false;
        const fresh = page.events.filter((e) => !seen.has(e.id));
        set((s) => ({
          prompts: s.prompts && {
            ...s.prompts,
            events: [...s.prompts.events, ...fresh],
            sessions: page.sessions,
          },
          morePrompts: fresh.length > 0 && page.events.length >= PAGE,
        }));
        return fresh.length > 0;
      } catch (e) {
        if (get().project === project) set({ error: String(e) });
        return false;
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
