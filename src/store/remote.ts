import { invoke, isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";
import { onLocaleApplied } from "../i18n";
import { type Pull, type RepoView, useGitHubStore } from "./github";
import { getPref, setPref } from "./prefs";
import { isRestoringProject, useProjectStore } from "./project";

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
  merge: boolean;
}

export interface RemoteView {
  origin: string;
  branch: string;
  branches: RemoteBranch[];
  commits: Commit[];
  /** The branch against the default branch; `null` on the default branch. */
  base: { name: string; ahead: number; behind: number } | null;
  view: RepoView;
}

export interface TreeFile {
  path: string;
  sha: string;
  mode: string;
  size: number;
}

export interface RemoteTree {
  head: string;
  files: TreeFile[];
  truncated: boolean;
}

/** An edited file: new `text`, or `null` to delete it. `original` is `null` for a new file. */
export interface Draft {
  text: string | null;
  original: string | null;
  mode: string | null;
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

  /** Files of the shown branch (loaded on demand). */
  tree: RemoteTree | null;
  treeLoading: boolean;
  /** Original text of files opened so far, by path. */
  loaded: Record<string, string>;
  drafts: Record<string, Draft>;
  openPath: string | null;
  loadTree: () => Promise<void>;
  openFile: (path: string) => Promise<void>;
  editFile: (path: string, text: string) => void;
  newFile: (path: string) => boolean;
  deleteFile: (path: string) => void;
  discard: (path: string) => void;
  /** Commits every draft; to `newBranch` when given. */
  commitFiles: (message: string, newBranch: string | null) => Promise<boolean>;
}

const noFiles = {
  tree: null,
  treeLoading: false,
  loaded: {},
  drafts: {},
  openPath: null,
};

/** The repository (and branch) picked last, restored when the app starts. */
const SAVED_KEY = "work.remoteRepo";

function loadSaved(): { slug: string; branch: string | null } | null {
  try {
    const saved = JSON.parse(getPref(SAVED_KEY) ?? "null");
    return typeof saved?.slug === "string"
      ? { slug: saved.slug, branch: typeof saved.branch === "string" ? saved.branch : null }
      : null;
  } catch {
    return null;
  }
}

function save(slug: string | null, branch: string | null = null) {
  setPref(SAVED_KEY, slug ? JSON.stringify({ slug, branch }) : null);
}

export const useRemoteStore = create<RemoteState>((set, get) => {
  let seq = 0;
  const saved = loadSaved();
  /** The saved branch, used for the first load only. */
  let savedBranch = saved?.branch ?? null;

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
    slug: saved?.slug ?? null,
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
      savedBranch = null;
      save(slug);
      set({ slug, data: null, error: null, loading: false, ...noFiles });
      void get().load();
    },

    close: () => {
      seq++;
      savedBranch = null;
      save(null);
      set({ slug: null, data: null, error: null, loading: false, ...noFiles });
    },

    load: async (branch) => {
      const { slug } = get();
      if (!slug || !isTauri()) return;
      const id = ++seq;
      set({ loading: true });
      try {
        const data = await invoke<RemoteView>("github_remote", {
          slug,
          branch: branch ?? get().data?.branch ?? savedBranch,
        });
        if (id !== seq) return;
        savedBranch = null;
        save(slug, data.branch);
        // Another branch: its own files.
        if (data.branch !== get().data?.branch) set(noFiles);
        set({ data, loading: false, error: null });
      } catch (e) {
        if (id !== seq) return;
        // The saved branch may be gone: fall back to the default branch.
        if (savedBranch && !branch) {
          savedBranch = null;
          return get().load();
        }
        set({ loading: false, error: String(e) });
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

    ...noFiles,

    loadTree: async () => {
      const { slug, data } = get();
      if (!slug || !data || !isTauri()) return;
      const branch = data.branch;
      set({ treeLoading: true });
      try {
        const tree = await invoke<RemoteTree>("github_remote_tree", { slug, branch });
        if (get().slug === slug && get().data?.branch === branch) {
          set({ tree, treeLoading: false });
        }
      } catch (e) {
        set({ treeLoading: false, error: String(e) });
      }
    },

    openFile: async (path) => {
      const { slug, tree, loaded, drafts } = get();
      set({ openPath: path });
      if (path in loaded || drafts[path]?.original === null) return;
      const file = tree?.files.find((f) => f.path === path);
      if (!slug || !file) return;
      try {
        const text = await invoke<string>("github_remote_file", { slug, sha: file.sha });
        set((s) => ({ loaded: { ...s.loaded, [path]: text } }));
      } catch (e) {
        set({ error: String(e), openPath: null });
      }
    },

    editFile: (path, text) =>
      set((s) => {
        const original = s.drafts[path] ? s.drafts[path].original : (s.loaded[path] ?? null);
        const mode = s.tree?.files.find((f) => f.path === path)?.mode ?? null;
        const drafts = { ...s.drafts };
        // Back to the original: no longer a change.
        if (original !== null && text === original) delete drafts[path];
        else drafts[path] = { text, original, mode };
        return { drafts };
      }),

    newFile: (path) => {
      const p = path.trim().replace(/\\/g, "/").replace(/^\/+/, "");
      if (!p || p.endsWith("/") || p.split("/").some((part) => part === "." || part === "..")) {
        return false;
      }
      if (get().tree?.files.some((f) => f.path === p) || get().drafts[p]) {
        void get().openFile(p);
        return true;
      }
      set((s) => ({
        drafts: { ...s.drafts, [p]: { text: "", original: null, mode: null } },
        openPath: p,
      }));
      return true;
    },

    deleteFile: (path) =>
      set((s) => {
        const drafts = { ...s.drafts };
        // A new file that was never committed just goes away.
        if (drafts[path]?.original === null) delete drafts[path];
        else {
          const mode = s.tree?.files.find((f) => f.path === path)?.mode ?? null;
          drafts[path] = { text: null, original: s.loaded[path] ?? "", mode };
        }
        return { drafts, openPath: s.openPath === path ? null : s.openPath };
      }),

    discard: (path) =>
      set((s) => {
        const drafts = { ...s.drafts };
        const wasNew = drafts[path]?.original === null;
        delete drafts[path];
        return { drafts, openPath: wasNew && s.openPath === path ? null : s.openPath };
      }),

    commitFiles: async (message, newBranch) => {
      const { slug, data, tree, drafts } = get();
      if (!slug || !data || !tree || !isTauri()) return false;
      const changes = Object.entries(drafts).map(([path, d]) => ({
        path,
        text: d.text,
        mode: d.mode,
      }));
      set({ busy: true, error: null });
      try {
        await invoke("github_remote_commit", {
          slug,
          branch: data.branch,
          expectedHead: tree.head,
          changes,
          message,
          newBranch,
        });
        set({ busy: false, ...noFiles });
        await get().load(newBranch ?? data.branch);
        await get().loadTree();
        return true;
      } catch (e) {
        set({ busy: false, error: String(e) });
        return false;
      }
    },
  };
});

// Errors come from the backend: drop the old-language one and reload.
onLocaleApplied(() => {
  useRemoteStore.setState({ error: null });
  void useRemoteStore.getState().load();
});

// Another account (or none) sees other repositories. The first status load
// at startup is not a change of account, so it keeps the saved repository.
useGitHubStore.subscribe((state, prev) => {
  if (prev.status && state.status?.account?.login !== prev.status.account?.login) {
    useRemoteStore.setState({ repos: null });
    useRemoteStore.getState().close();
  }
});

// Opening a local project leaves the GitHub-only view; reopening the last
// project at startup does not.
useProjectStore.subscribe((state, prev) => {
  if (state.current !== prev.current && !isRestoringProject()) useRemoteStore.getState().close();
});
