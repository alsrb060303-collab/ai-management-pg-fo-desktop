import { invoke, isTauri } from "@tauri-apps/api/core";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { create } from "zustand";
import { onLocaleApplied, t } from "../i18n";
import { useProjectStore } from "./project";

// Mirrors src-tauri/src/library/mod.rs.
export type ItemKind = "skill" | "agent";
export type Mode = "personal" | "team";
export type ApplyStatus = "current" | "outdated" | "modified" | "missing" | "conflict";

export interface LibraryItem {
  kind: ItemKind;
  id: string;
  name: string;
  description: string | null;
  path: string;
  mode: Mode | null;
  status: ApplyStatus | null;
}

export interface ToggledOff {
  kind: ItemKind;
  name: string;
  mode: Mode;
  /** Codex: the SKILL.md path that was turned off (Codex matches skills by path). */
  path: string | null;
}

export interface Overview {
  dir: string;
  isDefault: boolean;
  items: LibraryItem[];
  /** Items this app turned off in the selected project; only these can be turned back on. */
  toggledOff: ToggledOff[];
}

const MODIFIED = "MODIFIED:";

interface LibraryState {
  overview: Overview | null;
  busy: boolean;
  error: string | null;
  /** Reloads the overview; `keepError` leaves an action's error on screen. */
  load: (keepError?: boolean) => Promise<void>;
  chooseDir: () => Promise<void>;
  useDefaultDir: () => Promise<void>;
  apply: (item: LibraryItem, mode: Mode) => Promise<void>;
  remove: (item: LibraryItem) => Promise<void>;
  update: (item: LibraryItem) => Promise<void>;
  forget: (item: LibraryItem) => Promise<void>;
  importItem: (path: string, kind: ItemKind) => Promise<void>;
  /** `path` is the item's file; Codex needs it (it turns skills off by SKILL.md path). */
  setEnabled: (
    kind: ItemKind,
    name: string,
    enabled: boolean,
    mode: Mode,
    path?: string | null,
  ) => Promise<void>;
}

/** The project and tool every library command works on. */
function target() {
  const { current, tool } = useProjectStore.getState();
  return { project: current, tool };
}

export const useLibraryStore = create<LibraryState>((set, get) => {
  // Only the latest load may write, so a slow answer for an old project can't win.
  let loadId = 0;

  /** Runs a change, then reloads the library and the project's config map. */
  const run = async (action: () => Promise<unknown>) => {
    if (!isTauri()) return;
    set({ busy: true, error: null });
    try {
      await action();
    } catch (e) {
      set({ error: String(e).replace(MODIFIED, "") });
    } finally {
      // Stay busy until the views show the result, so buttons can't act on stale state.
      await Promise.all([get().load(true), useProjectStore.getState().refresh()]);
      set({ busy: false });
    }
  };

  /** Asks before discarding edits made in the project copy. */
  const withForce = async (command: string, item: LibraryItem, question: string) => {
    const args = { ...target(), kind: item.kind, id: item.id };
    try {
      await invoke(command, { ...args, force: false });
    } catch (e) {
      if (!String(e).startsWith(MODIFIED)) throw e;
      if (await ask(question, { title: item.name, kind: "warning" })) {
        await invoke(command, { ...args, force: true });
      }
    }
  };

  return {
    overview: null,
    busy: false,
    error: null,

    load: async (keepError = false) => {
      if (!isTauri()) return;
      const id = ++loadId;
      const { project, tool } = target();
      // Drop the old project's toggles right away; they don't apply to the new one.
      if (get().overview && !keepError) set({ overview: null, error: null });
      try {
        const overview = await invoke<Overview>("library_overview", { project, tool });
        if (id === loadId) set({ overview });
      } catch (e) {
        if (id === loadId) set({ error: String(e) });
      }
    },

    chooseDir: async () => {
      if (!isTauri()) return;
      const dir = await open({ directory: true, title: t("docs.library.chooseTitle") });
      if (typeof dir === "string") await run(() => invoke("library_set_dir", { dir }));
    },

    useDefaultDir: () => run(() => invoke("library_set_dir", { dir: null })),

    apply: (item, mode) =>
      run(() => invoke("library_apply", { ...target(), kind: item.kind, id: item.id, mode })),

    remove: (item) => run(() => withForce("library_remove", item, t("docs.library.confirmRemove"))),

    update: (item) => run(() => withForce("library_update", item, t("docs.library.confirmUpdate"))),

    forget: (item) =>
      run(() => invoke("library_forget", { ...target(), kind: item.kind, id: item.id })),

    importItem: (path, kind) =>
      run(() => invoke("library_import", { path, tool: target().tool, kind })),

    setEnabled: (kind, name, enabled, mode, path = null) =>
      run(() => invoke("config_set_enabled", { ...target(), kind, name, enabled, mode, path })),
  };
});

// Library errors come from the backend: reload in the new language.
onLocaleApplied(() => void useLibraryStore.getState().load());

// Statuses and toggles are per tool: reload when the project's tool changes.
useProjectStore.subscribe((state, prev) => {
  if (state.tool !== prev.tool) void useLibraryStore.getState().load();
});
