import { invoke, isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";
import { useProjectStore } from "./project";

// Mirrors src-tauri/src/work.
export interface Head {
  branch: string | null;
  commit: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
}

export type ChangeKind =
  | "modified"
  | "added"
  | "deleted"
  | "renamed"
  | "copied"
  | "typeChanged"
  | "untracked"
  | "conflicted";

export interface Change {
  path: string;
  origPath: string | null;
  kind: ChangeKind;
  staged: boolean;
  unstaged: boolean;
}

export interface Branch {
  name: string;
  current: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  gone: boolean;
  commit: string;
  subject: string;
  time: number;
  worktree: string | null;
}

export interface Worktree {
  path: string;
  branch: string | null;
  commit: string | null;
  isMain: boolean;
  isCurrent: boolean;
  managed: boolean;
  locked: boolean;
  prunable: boolean;
  changes: number | null;
}

export interface Overview {
  root: string;
  mainRoot: string;
  head: Head;
  operation: string | null;
  changes: Change[];
  changeCount: number;
  hasRemote: boolean;
  branches: Branch[];
  worktrees: Worktree[];
}

export interface Diff {
  text: string;
  binary: boolean;
  truncated: boolean;
}

interface WorkState {
  project: string | null;
  overview: Overview | null;
  /** The project is not a git repository. */
  notGit: boolean;
  /** git could not be run at all (checked when the project is not a repository). */
  gitMissing: boolean;
  loading: boolean;
  /** An operation is running; buttons are disabled meanwhile. */
  busy: boolean;
  error: string | null;
  /** The changed file whose diff is open. */
  selected: Change | null;
  diff: Diff | null;

  load: (project: string | null) => Promise<void>;
  /** Re-reads the state; `full` also counts changes in other worktrees. */
  refresh: (full?: boolean) => Promise<void>;
  select: (change: Change | null) => Promise<void>;
  dismissError: () => void;
  /** Commits only on this branch, or `null` when the check failed. */
  checkBranch: (name: string) => Promise<number | null>;
  /** Runs a git operation, then refreshes. Resolves whether it worked. */
  run: (command: string, args: Record<string, unknown>) => Promise<boolean>;
}

export const useWorkStore = create<WorkState>((set, get) => {
  let seq = 0;
  let diffSeq = 0;
  /** Refreshes running; a poll never supersedes (and so drops) one. */
  let inFlight = 0;

  const loadDiff = async () => {
    const { project, selected } = get();
    if (!project || !selected || !isTauri()) return;
    const id = ++diffSeq;
    try {
      const diff = await invoke<Diff>("work_diff", {
        project,
        path: selected.path,
        origPath: selected.origPath,
        untracked: selected.kind === "untracked",
      });
      if (id === diffSeq) set({ diff });
    } catch (e) {
      if (id === diffSeq) set({ diff: { text: String(e), binary: false, truncated: false } });
    }
  };

  return {
    project: null,
    overview: null,
    notGit: false,
    gitMissing: false,
    loading: false,
    busy: false,
    error: null,
    selected: null,
    diff: null,

    load: async (project) => {
      if (project !== get().project) {
        set({ project, overview: null, notGit: false, error: null, selected: null, diff: null });
      }
      if (!project) return;
      set({ loading: true, error: null });
      await get().refresh(true);
      set({ loading: false });
    },

    refresh: async (full = false) => {
      const { project } = get();
      if (!project || !isTauri() || (!full && inFlight > 0)) return;
      const id = ++seq;
      inFlight++;
      try {
        const next = await invoke<Overview | null>("work_overview", {
          project,
          countWorktrees: full,
        }).finally(() => inFlight--);
        if (id !== seq || get().project !== project) return;
        // A poll does not count other worktrees; keep the last counts.
        if (next && !full) {
          const old = get().overview?.worktrees ?? [];
          for (const wt of next.worktrees) {
            const prev = old.find((o) => o.path === wt.path && o.commit === wt.commit);
            wt.changes ??= prev?.changes ?? null;
          }
        }
        const selected = get().selected;
        const still = selected && next?.changes.find((c) => c.path === selected.path);
        if (next === null && !get().notGit) {
          void invoke<string | null>("work_git_version", { project }).then((version) => {
            if (get().project === project) set({ gitMissing: version === null });
          });
        }
        set({
          overview: next,
          notGit: next === null,
          ...(selected && !still ? { selected: null, diff: null } : {}),
        });
        if (still) {
          set({ selected: still });
          await loadDiff();
        }
      } catch (e) {
        // Polls keep what is shown; a full refresh reports the problem.
        if (id === seq && full) set({ error: String(e) });
      }
    },

    select: async (change) => {
      if (!change || change.path === get().selected?.path) {
        set({ selected: null, diff: null });
        return;
      }
      set({ selected: change, diff: null });
      await loadDiff();
    },

    dismissError: () => set({ error: null }),

    checkBranch: async (name) => {
      const { project } = get();
      if (!project || !isTauri()) return null;
      try {
        const check = await invoke<{ unmerged: number }>("work_check_branch", { project, name });
        return check.unmerged;
      } catch (e) {
        set({ error: String(e) });
        return null;
      }
    },

    run: async (command, args) => {
      const { project } = get();
      if (!project || !isTauri() || get().busy) return false;
      set({ busy: true, error: null });
      let ok = true;
      try {
        await invoke(command, { project, ...args });
      } catch (e) {
        ok = false;
        set({ error: String(e) });
      }
      set({ busy: false });
      await get().refresh(true);
      // Switching branches can change the project's Claude Code files.
      if (command === "work_switch" || command === "work_create_branch") {
        void useProjectStore.getState().refresh();
      }
      return ok;
    },
  };
});
