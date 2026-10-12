import { invoke, isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";
import { onLocaleApplied } from "../i18n";
import { type Pull, type RepoView, useGitHubStore } from "./github";
import { useProjectStore } from "./project";

// Mirrors src-tauri/src/github (api.rs).
export interface RepoSummary {
  /** `owner/repo`. */
  slug: string;
  description: string | null;
  private: boolean;
  fork: boolean;
  defaultBranch: string;
  url: string;
  pushedAt: string | null;
}

export interface RemoteBranch {
  name: string;
  protected: boolean;
}

export interface Commit {
  sha: string;
  subject: string;
  author: string;
  date: string;
  url: string;
}

export interface RemoteView {
  origin: string;
  branch: string;
  branches: RemoteBranch[];
  commits: Commit[];
  view: RepoView;
}

interface LocalClone {
  slug: string;
  path: string;
}

interface RemoteState {
  /** The signed-in account's repositories (`null` until loaded). */
  repos: RepoSummary[] | null;
  reposLoading: boolean;
  /** Lowercase `owner/repo` → a local clone among the recent projects. */
  clones: Record<string, string>;
  /** A repository shown from GitHub only (not cloned); `null` shows the local project. */
  slug: string | null;
  data: RemoteView | null;
  loading: boolean;
  busy: boolean;
  error: string | null;

  loadRepos: (force?: boolean) => Promise<void>;
  findClones: (paths: string[]) => Promise<void>;
  open: (slug: string) => void;
  close: () => void;
  load: (branch?: string) => Promise<void>;
  createBranch: (name: string, from: string) => Promise<boolean>;
  deleteBranch: (name: string) => Promise<void>;
  createPr: (input: {
    head: string;
    base: string;
    title: string;
    body: string;
    draft: boolean;
  }) => Promise<Pull | null>;
  /** Clones the open repository under `parent`; returns the new folder. */
  clone: (parent: string) => Promise<string | null>;
  dismissError: () => void;
}

export const useRemoteStore = create<RemoteState>((set, get) => {
  let seq = 0;

  /** Runs a change, then reloads the view; returns whether it worked. */
  const change = async (run: () => Promise<unknown>, branch?: string) => {
    set({ busy: true, error: null });
    try {
      await run();
      set({ busy: false });
      await get().load(branch);
      return true;
    } catch (e) {
      set({ busy: false, error: String(e) });
      return false;
    }
  };

  return {
    repos: null,
    reposLoading: false,
    clones: {},
    slug: null,
    data: null,
    loading: false,
    busy: false,
    error: null,

    loadRepos: async (force = false) => {
      if (!isTauri() || get().reposLoading || (get().repos && !force)) return;
      set({ reposLoading: true });
      try {
        set({ repos: await invoke<RepoSummary[]>("github_repos"), reposLoading: false });
      } catch (e) {
        set({ error: String(e), reposLoading: false });
      }
    },

    findClones: async (paths) => {
      if (!isTauri() || !paths.length) return;
      const found = await invoke<LocalClone[]>("github_local_clones", { paths });
      const clones: Record<string, string> = {};
      for (const c of found) clones[c.slug] ??= c.path;
      set({ clones });
    },

    open: (slug) => {
      if (get().slug === slug) return;
      seq++;
      set({ slug, data: null, error: null, loading: false });
      void get().load();
    },

    close: () => {
      seq++;
      set({ slug: null, data: null, error: null, loading: false });
    },

    load: async (branch) => {
      const { slug } = get();
      if (!slug || !isTauri()) return;
      const id = ++seq;
      set({ loading: true });
      try {
        const data = await invoke<RemoteView>("github_remote", {
          slug,
          branch: branch ?? get().data?.branch ?? null,
        });
        if (id === seq) set({ data, loading: false, error: null });
      } catch (e) {
        if (id === seq) set({ loading: false, error: String(e) });
      }
    },

    createBranch: (name, from) =>
      change(
        () => invoke("github_remote_create_branch", { slug: get().slug, name, from }),
        name.trim(),
      ),

    deleteBranch: async (name) => {
      const data = get().data;
      // Deleting the shown branch goes back to the default one.
      const next = data?.branch === name ? data.view.defaultBranch : undefined;
      await change(() => invoke("github_remote_delete_branch", { slug: get().slug, name }), next);
    },

    createPr: async (input) => {
      let pull: Pull | null = null;
      await change(async () => {
        pull = await invoke<Pull>("github_remote_create_pr", { slug: get().slug, ...input });
      });
      return pull;
    },

    clone: async (parent) => {
      const { slug } = get();
      if (!slug || !isTauri()) return null;
      set({ busy: true, error: null });
      try {
        const path = await invoke<string>("github_clone", { slug, parent });
        set((s) => ({ busy: false, clones: { ...s.clones, [slug.toLowerCase()]: path } }));
        return path;
      } catch (e) {
        set({ busy: false, error: String(e) });
        return null;
      }
    },

    dismissError: () => set({ error: null }),
  };
});

// Errors come from the backend: drop the old-language one and reload.
onLocaleApplied(() => {
  useRemoteStore.setState({ error: null });
  void useRemoteStore.getState().load();
});

// Another account (or none) sees other repositories.
useGitHubStore.subscribe((state, prev) => {
  if (state.status?.account?.login !== prev.status?.account?.login) {
    useRemoteStore.setState({ repos: null });
    useRemoteStore.getState().close();
  }
});

// Opening a local project leaves the GitHub-only view.
useProjectStore.subscribe((state, prev) => {
  if (state.current !== prev.current) useRemoteStore.getState().close();
});
