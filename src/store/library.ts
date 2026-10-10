import { invoke, isTauri } from "@tauri-apps/api/core";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { create } from "zustand";
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
  setEnabled: (kind: ItemKind, name: string, enabled: boolean, mode: Mode) => Promise<void>;
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
    const project = useProjectStore.getState().current;
    const args = { project, kind: item.kind, id: item.id };
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
      const project = useProjectStore.getState().current;
      // Drop the old project's toggles right away; they don't apply to the new one.
      if (get().overview && !keepError) set({ overview: null, error: null });
      try {
        const overview = await invoke<Overview>("library_overview", { project });
        if (id === loadId) set({ overview });
      } catch (e) {
        if (id === loadId) set({ error: String(e) });
      }
    },

    chooseDir: async () => {
      if (!isTauri()) return;
      const dir = await open({ directory: true, title: "라이브러리 폴더 선택" });
      if (typeof dir === "string") await run(() => invoke("library_set_dir", { dir }));
    },

    useDefaultDir: () => run(() => invoke("library_set_dir", { dir: null })),

    apply: (item, mode) =>
      run(() =>
        invoke("library_apply", {
          project: useProjectStore.getState().current,
          kind: item.kind,
          id: item.id,
          mode,
        }),
      ),

    remove: (item) =>
      run(() =>
        withForce(
          "library_remove",
          item,
          "프로젝트에 있는 복사본이 수정되었습니다. 수정한 내용을 버리고 뺄까요?",
        ),
      ),

    update: (item) =>
      run(() =>
        withForce(
          "library_update",
          item,
          "프로젝트에 있는 복사본이 수정되었습니다. 라이브러리 버전으로 덮어쓸까요?",
        ),
      ),

    forget: (item) =>
      run(() =>
        invoke("library_forget", {
          project: useProjectStore.getState().current,
          kind: item.kind,
          id: item.id,
        }),
      ),

    importItem: (path, kind) => run(() => invoke("library_import", { path, kind })),

    setEnabled: (kind, name, enabled, mode) =>
      run(() =>
        invoke("config_set_enabled", {
          project: useProjectStore.getState().current,
          kind,
          name,
          enabled,
          mode,
        }),
      ),
  };
});
