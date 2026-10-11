import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

// Mirrors src-tauri/src/memo.
export interface Todo {
  id: string;
  text: string;
  done: boolean;
  position: number;
  createdAt: number;
  updatedAt: number;
}

export interface BranchInfo {
  name: string;
  exists: boolean;
  current: boolean;
  open: number;
  total: number;
  hasNote: boolean;
}

export interface MemoContext {
  repoKey: string;
  repoName: string;
  isGit: boolean;
  /** Checked-out branch; "" outside git or on a detached HEAD. */
  current: string;
  detachedAt: string | null;
  branches: BranchInfo[];
}

interface BranchMemo {
  note: string;
  noteUpdatedAt: number | null;
  todos: Todo[];
}

interface MemoState {
  project: string | null;
  context: MemoContext | null;
  /** Branch picked by the user; `null` follows the checked-out branch. */
  picked: string | null;
  note: string;
  /** The note has edits not yet saved. */
  noteDirty: boolean;
  todos: Todo[];
  loading: boolean;
  error: string | null;

  load: (project: string | null) => Promise<void>;
  /** Re-reads the checked-out branch; switches when following it. */
  poll: () => Promise<void>;
  pick: (branch: string | null) => void;
  setNote: (text: string) => void;
  flushNote: () => Promise<void>;
  addTodo: (text: string) => Promise<void>;
  toggleTodo: (id: string) => Promise<void>;
  editTodo: (id: string, text: string) => Promise<void>;
  deleteTodo: (id: string) => Promise<void>;
  moveTodo: (id: string, index: number) => Promise<void>;
  clearDone: () => Promise<void>;
}

/** The branch the panel shows. */
export function shownBranch(s: Pick<MemoState, "context" | "picked">): string | null {
  if (!s.context) return null;
  return s.picked ?? s.context.current;
}

const NOTE_SAVE_MS = 500;

export const useMemoStore = create<MemoState>((set, get) => {
  let contextSeq = 0;
  let branchSeq = 0;
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  /** Where the pending note edit belongs, captured when it was typed. */
  let noteTarget: { repoKey: string; branch: string } | null = null;

  const key = () => {
    const s = get();
    const branch = shownBranch(s);
    return s.context && branch !== null ? { repoKey: s.context.repoKey, branch } : null;
  };

  const fail = (e: unknown) => set({ error: String(e) });

  const loadBranch = async () => {
    const k = key();
    if (!k || !isTauri()) return;
    const seq = ++branchSeq;
    try {
      const memo = await invoke<BranchMemo>("memo_get", k);
      if (seq !== branchSeq) return;
      // Keep what the user is typing; the save will catch up.
      set((s) => ({ todos: memo.todos, ...(s.noteDirty ? {} : { note: memo.note }) }));
    } catch (e) {
      if (seq === branchSeq) fail(e);
    }
  };

  const refreshCounts = async () => {
    const { project } = get();
    if (!project || !isTauri()) return;
    const seq = ++contextSeq;
    try {
      const context = await invoke<MemoContext>("memo_context", { project });
      if (seq === contextSeq && get().project === project) set({ context });
    } catch {
      // Counts are cosmetic; the next load reports real errors.
    }
  };

  /** Runs a change, then reloads (also undoing an optimistic update on error). */
  const mutate = async (command: string, args: Record<string, unknown>) => {
    try {
      await invoke(command, args);
    } catch (e) {
      fail(e);
    }
    await Promise.all([loadBranch(), refreshCounts()]);
  };

  if (isTauri()) {
    void listen<{ repoKey: string; branch: string }>("memo-changed", (event) => {
      const k = key();
      if (k && event.payload.repoKey === k.repoKey) {
        if (event.payload.branch === k.branch) void loadBranch();
        void refreshCounts();
      }
    });
  }

  return {
    project: null,
    context: null,
    picked: null,
    note: "",
    noteDirty: false,
    todos: [],
    loading: false,
    error: null,

    load: async (project) => {
      await get().flushNote();
      if (project !== get().project) {
        set({ project, context: null, picked: null, note: "", todos: [], error: null });
      }
      if (!project || !isTauri()) return;
      const seq = ++contextSeq;
      set({ loading: true, error: null });
      try {
        const context = await invoke<MemoContext>("memo_context", { project });
        if (seq !== contextSeq || get().project !== project) return;
        set({ context, loading: false });
        await loadBranch();
      } catch (e) {
        if (seq === contextSeq) set({ loading: false, error: String(e) });
      }
    },

    poll: async () => {
      const { project, context, picked } = get();
      if (!project || !context || !isTauri()) return;
      try {
        const next = await invoke<MemoContext>("memo_context", { project });
        if (get().project !== project) return;
        const switched = picked === null && next.current !== context.current;
        if (switched) await get().flushNote();
        set({ context: next, ...(switched ? { note: "", todos: [] } : {}) });
        if (switched) await loadBranch();
      } catch {
        // Polling is best effort.
      }
    },

    pick: (branch) => {
      void get()
        .flushNote()
        .then(() => {
          const current = get().context?.current;
          set({ picked: branch === current ? null : branch, note: "", todos: [] });
          return loadBranch();
        });
    },

    setNote: (text) => {
      // A pending edit for one branch is never saved into another.
      noteTarget = noteTarget ?? key();
      set({ note: text, noteDirty: true });
      clearTimeout(noteTimer);
      noteTimer = setTimeout(() => void get().flushNote(), NOTE_SAVE_MS);
    },

    flushNote: async () => {
      clearTimeout(noteTimer);
      const target = noteTarget;
      noteTarget = null;
      if (!get().noteDirty || !target || !isTauri()) return;
      const text = get().note;
      set({ noteDirty: false });
      try {
        await invoke("memo_set_note", { ...target, text });
      } catch (e) {
        fail(e);
      }
    },

    addTodo: async (text) => {
      const k = key();
      if (!k || !text.trim()) return;
      await mutate("memo_add_todo", { ...k, text });
    },

    toggleTodo: async (id) => {
      const todo = get().todos.find((t) => t.id === id);
      if (!todo) return;
      set({ todos: get().todos.map((t) => (t.id === id ? { ...t, done: !t.done } : t)) });
      await mutate("memo_update_todo", { id, done: !todo.done });
    },

    editTodo: async (id, text) => {
      if (!text.trim()) return;
      set({ todos: get().todos.map((t) => (t.id === id ? { ...t, text: text.trim() } : t)) });
      await mutate("memo_update_todo", { id, text });
    },

    deleteTodo: async (id) => {
      set({ todos: get().todos.filter((t) => t.id !== id) });
      await mutate("memo_delete_todo", { id });
    },

    moveTodo: async (id, index) => {
      const todos = [...get().todos];
      const from = todos.findIndex((t) => t.id === id);
      if (from === -1 || from === index) return;
      const [moved] = todos.splice(from, 1);
      todos.splice(index, 0, moved);
      set({ todos });
      await mutate("memo_move_todo", { id, index });
    },

    clearDone: async () => {
      const k = key();
      if (!k) return;
      set({ todos: get().todos.filter((t) => !t.done) });
      await mutate("memo_clear_done", k);
    },
  };
});
