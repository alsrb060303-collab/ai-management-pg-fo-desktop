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

export interface Overview {
  dir: string;
  isDefault: boolean;
  items: LibraryItem[];
}

const MODIFIED = "MODIFIED:";

interface LibraryState {
  overview: Overview | null;
  busy: boolean;
  error: string | null;
  load: () => Promise<void>;
  chooseDir: () => Promise<void>;
  useDefaultDir: () => Promise<void>;
  apply: (item: LibraryItem, mode: Mode) => Promise<void>;
  remove: (item: LibraryItem) => Promise<void>;
  update: (item: LibraryItem) => Promise<void>;
  importItem: (path: string, kind: ItemKind) => Promise<void>;
  setEnabled: (kind: ItemKind, name: string, enabled: boolean, mode: Mode) => Promise<void>;
}

export const useLibraryStore = create<LibraryState>((set, get) => {
  /** Runs a change, then reloads the library and the project's config map. */
  const run = async (action: () => Promise<unknown>) => {
    if (!isTauri()) return;
    set({ busy: true, error: null });
    try {
      await action();
    } catch (e) {
      set({ error: String(e).replace(MODIFIED, "") });
    }
    set({ busy: false });
    await Promise.all([get().load(), useProjectStore.getState().refresh()]);
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

    load: async () => {
      if (!isTauri()) return;
      const project = useProjectStore.getState().current;
      try {
        const overview = await invoke<Overview>("library_overview", { project });
        set({ overview });
      } catch (e) {
        set({ error: String(e) });
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
