import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { t } from "../i18n";

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
  pick: (branch: string | null) => Promise<void>;
  setNote: (text: string) => void;
  flushNote: () => Promise<void>;
  /** Resolves `false` when the TODO was not saved. */
  addTodo: (text: string) => Promise<boolean>;
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

interface Key {
  repoKey: string;
  branch: string;
}

const sameKey = (a: Key | null, b: Key | null) =>
  a?.repoKey === b?.repoKey && a?.branch === b?.branch;

export const useMemoStore = create<MemoState>((set, get) => {
  let contextSeq = 0;
  let branchSeq = 0;
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  /** Where the pending note edit belongs, captured when it was typed. */
  let noteTarget: Key | null = null;
  /** The note-save error on screen, so a later successful save can clear just that one. */
  let noteError: string | null = null;

  const keyOf = (s: Pick<MemoState, "context" | "picked">): Key | null => {
    const branch = shownBranch(s);
    return s.context && branch !== null ? { repoKey: s.context.repoKey, branch } : null;
  };
  const key = () => keyOf(get());

  const fail = (e: unknown) => set({ error: String(e) });

  /**
   * Takes the unsaved note edit off the panel. Synchronous, so a caller that
   * also switches branches in the same tick cannot lose or misfile a keystroke.
   */
  const takeNote = () => {
    clearTimeout(noteTimer);
    const target = noteTarget;
    noteTarget = null;
    if (!get().noteDirty || !target) return null;
    set({ noteDirty: false });
    return { target, text: get().note };
  };

  const saveNote = async (pending: { target: Key; text: string } | null) => {
    if (!pending || !isTauri()) return;
    try {
      await invoke("memo_set_note", { ...pending.target, text: pending.text });
      if (noteError !== null && get().error === noteError) set({ error: null });
      noteError = null;
    } catch (e) {
      // Still showing that note and nothing newer typed: keep it unsaved, so
      // the next edit, blur or switch tries again.
      if (noteTarget === null && sameKey(key(), pending.target)) {
        noteTarget = pending.target;
        set({ noteDirty: true });
      }
      noteError = t("memo.note.saveFailed", { error: String(e) });
      set({ error: noteError });
    }
  };

  const loadBranch = async () => {
    const k = key();
    if (!k || !isTauri()) return;
    const seq = ++branchSeq;
    try {
      const memo = await invoke<BranchMemo>("memo_get", { ...k });
      if (seq !== branchSeq) return;
      // Keep what the user is typing into this branch; the save will catch up.
      const typing = get().noteDirty && sameKey(noteTarget, k);
      set({ todos: memo.todos, ...(typing ? {} : { note: memo.note }) });
    } catch (e) {
      if (seq === branchSeq) fail(e);
    }
  };

  /** Changes what the panel shows, saving the note of what it showed. */
  const switchTo = async (changes: Partial<MemoState>) => {
    const pending = takeNote();
    set({ ...changes, note: "", todos: [] });
    await Promise.all([saveNote(pending), loadBranch()]);
  };

  /** The project's context, or `null` when a newer request or project won. */
  const fetchContext = async (project: string) => {
    const seq = ++contextSeq;
    const context = await invoke<MemoContext>("memo_context", { project });
    return seq === contextSeq && get().project === project ? context : null;
  };

  /** Every context update goes through here: a new one can move the shown branch. */
  const applyContext = async (context: MemoContext) => {
    const s = get();
    if (sameKey(keyOf(s), keyOf({ context, picked: s.picked }))) set({ context });
    else await switchTo({ context });
  };

  const refreshCounts = async () => {
    const { project } = get();
    if (!project || !isTauri()) return;
    try {
      const context = await fetchContext(project);
      if (context) await applyContext(context);
    } catch {
      // Counts are cosmetic; the next load reports real errors.
    }
  };

  /** Runs a change, then reloads (also undoing an optimistic update on error). */
  const mutate = async (command: string, args: Record<string, unknown>) => {
    let ok = true;
    try {
      await invoke(command, args);
    } catch (e) {
      ok = false;
      fail(e);
    }
    await Promise.all([loadBranch(), refreshCounts()]);
    return ok;
  };

  if (isTauri()) {
    void listen<Key>("memo-changed", (event) => {
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
      if (project !== get().project) {
        await switchTo({ project, context: null, picked: null, error: null });
      } else {
        await saveNote(takeNote());
      }
      if (!project || !isTauri()) return;
      set({ loading: true, error: null });
      try {
        const context = await fetchContext(project);
        if (get().project === project) set({ loading: false });
        if (context) await applyContext(context);
      } catch (e) {
        if (get().project === project) set({ loading: false, error: String(e) });
      }
    },

    poll: async () => {
      const { project, context } = get();
      if (!project || !context || !isTauri()) return;
      try {
        const next = await fetchContext(project);
        if (next) await applyContext(next);
      } catch {
        // Polling is best effort; keep showing what we have.
      }
    },

    pick: async (branch) => {
      const current = get().context?.current;
      const picked = branch === current ? null : branch;
      if (picked === get().picked) return;
      await switchTo({ picked });
    },

    setNote: (text) => {
      // A pending edit for one branch is never saved into another.
      noteTarget = noteTarget ?? key();
      set({ note: text, noteDirty: true });
      clearTimeout(noteTimer);
      noteTimer = setTimeout(() => void get().flushNote(), NOTE_SAVE_MS);
    },

    flushNote: () => saveNote(takeNote()),

    addTodo: async (text) => {
      const k = key();
      if (!k || !text.trim()) return false;
      return mutate("memo_add_todo", { ...k, text });
    },
    toggleTodo: async (id) => {
      const todo = get().todos.find((item) => item.id === id);
      if (!todo) return;
      set({
        todos: get().todos.map((item) => (item.id === id ? { ...item, done: !item.done } : item)),
      });
      await mutate("memo_update_todo", { id, done: !todo.done });
    },

    editTodo: async (id, text) => {
      if (!text.trim()) return;
      set({
        todos: get().todos.map((item) => (item.id === id ? { ...item, text: text.trim() } : item)),
      });
      await mutate("memo_update_todo", { id, text });
    },

    deleteTodo: async (id) => {
      set({ todos: get().todos.filter((item) => item.id !== id) });
      await mutate("memo_delete_todo", { id });
    },

    moveTodo: async (id, index) => {
      const todos = [...get().todos];
      const from = todos.findIndex((item) => item.id === id);
      if (from === -1 || from === index) return;
      const [moved] = todos.splice(from, 1);
      todos.splice(index, 0, moved);
      set({ todos });
      await mutate("memo_move_todo", { id, index });
    },

    clearDone: async () => {
      const k = key();
      if (!k) return;
      set({ todos: get().todos.filter((item) => !item.done) });
      await mutate("memo_clear_done", { ...k });
    },
  };
});
