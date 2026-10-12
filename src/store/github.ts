import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { onLocaleApplied } from "../i18n";

// Mirrors src-tauri/src/github.
export interface Account {
  login: string;
  name: string | null;
}

export interface Status {
  configured: boolean;
  account: Account | null;
  offline: boolean;
}

export interface DeviceCode {
  userCode: string;
  verificationUri: string;
  expiresIn: number;
}

export interface Pull {
  number: number;
  title: string;
  state: "open" | "closed" | "merged";
  draft: boolean;
  url: string;
  author: string;
  head: string;
  base: string;
  updatedAt: string;
}

export interface Issue {
  number: number;
  title: string;
  url: string;
  author: string;
  labels: string[];
  comments: number;
  updatedAt: string;
}

export interface Checks {
  passed: number;
  failed: number;
  pending: number;
  skipped: number;
}

export interface RepoView {
  slug: string;
  url: string;
  defaultBranch: string;
  pushed: boolean;
  current: Pull | null;
  checks: Checks | null;
  pulls: Pull[];
  issues: Issue[];
}

interface GitHubState {
  status: Status | null;
  /** A sign-in waiting for the user to enter the code on github.com. */
  login: DeviceCode | null;
  /** Why the last sign-in did not finish. */
  loginError: string | null;
  project: string | null;
  /** `null` when the project is not on github.com (or not loaded yet). */
  view: RepoView | null;
  loading: boolean;
  error: string | null;

  init: () => Promise<void>;
  startLogin: () => Promise<void>;
  cancelLogin: () => void;
  logout: () => Promise<void>;
  load: (project: string | null) => Promise<void>;
  createPr: (input: {
    base: string;
    title: string;
    body: string;
    draft: boolean;
  }) => Promise<Pull | null>;
}

function summarize(view: RepoView | null) {
  if (!view || (!view.current && !view.checks)) return null;
  return {
    prNumber: view.current?.number ?? null,
    prTitle: view.current?.title ?? null,
    prState: view.current ? (view.current.draft ? "draft" : view.current.state) : null,
    prUrl: view.current?.url ?? null,
    passed: view.checks?.passed ?? 0,
    failed: view.checks?.failed ?? 0,
    pending: view.checks?.pending ?? 0,
  };
}

export const useGitHubStore = create<GitHubState>((set, get) => {
  let seq = 0;
  let loginSeq = 0;
  let listening = false;
  /** A sign-in request is on its way (a double click starts only one). */
  let starting = false;

  const refreshStatus = async () => {
    if (!isTauri()) return;
    try {
      set({ status: await invoke<Status>("github_status") });
    } catch (e) {
      set({ error: String(e) });
    }
  };

  return {
    status: null,
    login: null,
    loginError: null,
    project: null,
    view: null,
    loading: false,
    error: null,

    init: async () => {
      if (!isTauri()) return;
      if (!listening) {
        listening = true;
        void listen<{ signedIn: boolean; message: string | null }>("github-auth", (event) => {
          set({ login: null, loginError: event.payload.message });
          void refreshStatus().then(() => {
            if (event.payload.signedIn) void get().load(get().project);
            else set({ view: null });
          });
        });
      }
      await refreshStatus();
    },

    startLogin: async () => {
      if (starting) return;
      starting = true;
      const id = ++loginSeq;
      set({ loginError: null });
      try {
        const login = await invoke<DeviceCode>("github_login_start");
        if (id === loginSeq) set({ login });
      } catch (e) {
        if (id === loginSeq) set({ loginError: String(e) });
      } finally {
        starting = false;
      }
    },

    cancelLogin: () => {
      loginSeq++;
      set({ login: null });
      if (isTauri()) void invoke("github_login_cancel");
    },

    logout: async () => {
      try {
        await invoke("github_logout");
      } catch (e) {
        set({ error: String(e) });
      }
      set({ view: null });
      await refreshStatus();
    },

    load: async (project) => {
      if (project !== get().project) set({ project, view: null, error: null });
      // Signed in but offline at the last check: see whether GitHub is back.
      if (get().status?.offline) await refreshStatus();
      const id = ++seq;
      if (!project || !isTauri() || !get().status?.account) {
        set({ loading: false });
        return;
      }
      set({ loading: true });
      try {
        const view = await invoke<RepoView | null>("github_repo", { project });
        if (id === seq) set({ view, error: null, loading: false });
        // The Claude Code mod shows the branch's PR and CI too.
        void invoke("bridge_github", { project, summary: summarize(view) }).catch(() => {});
      } catch (e) {
        if (id === seq) set({ error: String(e), loading: false });
      }
    },

    createPr: async (input) => {
      const { project } = get();
      if (!project || !isTauri()) return null;
      try {
        const pull = await invoke<Pull>("github_create_pr", { project, ...input });
        set({ error: null });
        await get().load(project);
        return pull;
      } catch (e) {
        set({ error: String(e) });
        return null;
      }
    },
  };
});

// GitHub errors come from the backend: drop the old-language error and reload.
onLocaleApplied(() => {
  useGitHubStore.setState({ error: null, loginError: null });
  void useGitHubStore.getState().load(useGitHubStore.getState().project);
});
