import { invoke, isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";
import { onLocaleApplied } from "../i18n";
import { useProjectStore } from "./project";

// Mirrors src-tauri/src/project_docs.rs.
export type DocKind =
  | "instructions"
  | "rule"
  | "skill"
  | "agent"
  | "command"
  | "settings"
  | "mcp"
  | "other";

export interface DocEntry {
  rel: string;
  kind: DocKind;
  size: number;
  modified: number;
}

/** The backend's answer when the file changed since it was opened. */
const CONFLICT = "conflict";

interface ProjectDocsState {
  project: string | null;
  entries: DocEntry[];
  loading: boolean;
  error: string | null;
  /** The open file. */
  rel: string | null;
  text: string;
  /** The text as on disk when opened or saved. */
  saved: string;
  /** The change time the editor's text is based on. */
  modified: number | null;
  /** The open file changed on disk while it had unsaved edits. */
  changedOutside: boolean;
  saving: boolean;

  load: (project: string | null) => Promise<void>;
  /** Re-lists and picks up outside changes to the open file. */
  refresh: () => Promise<void>;
  open: (rel: string) => Promise<boolean>;
  edit: (text: string) => void;
  save: (force?: boolean) => Promise<boolean>;
  /** Drops unsaved edits and reads the file again. */
  reload: () => Promise<void>;
  create: (rel: string, text: string) => Promise<boolean>;
  remove: (rel: string) => Promise<void>;
  close: () => void;
  dismissError: () => void;
}

export const useProjectDocsStore = create<ProjectDocsState>((set, get) => {
  let seq = 0;

  const read = async (rel: string) => {
    const project = get().project;
    return invoke<{ text: string; modified: number }>("project_doc_read", { project, rel });
  };

  return {
    project: null,
    entries: [],
    loading: false,
    error: null,
    rel: null,
    text: "",
    saved: "",
    modified: null,
    changedOutside: false,
    saving: false,

    load: async (project) => {
      if (project !== get().project) {
        set({
          project,
          entries: [],
          error: null,
          rel: null,
          text: "",
          saved: "",
          modified: null,
          changedOutside: false,
        });
      }
      await get().refresh();
    },

    refresh: async () => {
      const { project } = get();
      if (!project || !isTauri()) return;
      const id = ++seq;
      set({ loading: !get().entries.length });
      try {
        const entries = await invoke<DocEntry[]>("project_docs_list", { project });
        if (id !== seq) return;
        set({ entries, loading: false });
        const { rel, modified, text, saved } = get();
        if (!rel) return;
        const entry = entries.find((e) => e.rel === rel);
        if (!entry) {
          // Deleted outside: keep unsaved text so it can be saved again.
          if (text === saved) get().close();
          return;
        }
        if (entry.modified !== modified) {
          if (text === saved) await get().reload();
          else set({ changedOutside: true });
        }
      } catch (e) {
        if (id === seq) set({ error: String(e), loading: false });
      }
    },

    open: async (rel) => {
      const { text, saved } = get();
      if (rel === get().rel) return true;
      if (text !== saved) return false;
      try {
        const doc = await read(rel);
        set({
          rel,
          text: doc.text,
          saved: doc.text,
          modified: doc.modified,
          changedOutside: false,
          error: null,
        });
        return true;
      } catch (e) {
        set({ error: String(e) });
        return false;
      }
    },

    edit: (text) => set({ text }),

    save: async (force = false) => {
      const { project, rel, text, modified } = get();
      if (!project || !rel || !isTauri()) return false;
      set({ saving: true });
      try {
        const at = await invoke<number>("project_doc_write", {
          project,
          rel,
          text,
          expected: modified,
          force,
        });
        set({ saved: text, modified: at, changedOutside: false, saving: false, error: null });
        void get().refresh();
        // The settings map reads the same files.
        if (useProjectStore.getState().current === project) {
          void useProjectStore.getState().refresh();
        }
        return true;
      } catch (e) {
        const error = String(e);
        if (error === CONFLICT) set({ changedOutside: true, saving: false });
        else set({ error, saving: false });
        return false;
      }
    },

    reload: async () => {
      const { rel } = get();
      if (!rel) return;
      try {
        const doc = await read(rel);
        set({ text: doc.text, saved: doc.text, modified: doc.modified, changedOutside: false });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    create: async (rel, text) => {
      const { project } = get();
      if (!project || !isTauri()) return false;
      try {
        const at = await invoke<number>("project_doc_write", {
          project,
          rel,
          text,
          expected: null,
          force: false,
        });
        set({ rel, text, saved: text, modified: at, changedOutside: false, error: null });
        await get().refresh();
        void useProjectStore.getState().refresh();
        return true;
      } catch (e) {
        const error = String(e);
        set({ error: error === CONFLICT ? null : error });
        // Already there: open it instead.
        if (error === CONFLICT) return get().open(rel);
        return false;
      }
    },

    remove: async (rel) => {
      const { project } = get();
      if (!project) return;
      try {
        await invoke("project_doc_delete", { project, rel });
        if (get().rel === rel) get().close();
        await get().refresh();
        void useProjectStore.getState().refresh();
      } catch (e) {
        set({ error: String(e) });
      }
    },

    close: () => set({ rel: null, text: "", saved: "", modified: null, changedOutside: false }),

    dismissError: () => set({ error: null }),
  };
});

onLocaleApplied(() => useProjectDocsStore.setState({ error: null }));
