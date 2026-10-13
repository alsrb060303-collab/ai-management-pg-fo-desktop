import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { useLibraryStore } from "./library";

// Mirrors src-tauri/src/sync.
export type SyncState =
  | "notConfigured"
  | "off"
  | "needsSignIn"
  | "notAllowed"
  | "syncing"
  | "idle"
  | "error";

export interface SyncStatus {
  state: SyncState;
  uid: string | null;
  /** Epoch ms of the last completed sync. */
  lastSync: number | null;
  message: string | null;
}

export interface Conflict {
  kind: "doc" | "skill" | "agent";
  name: string;
  copy: string | null;
  revived: boolean;
}

/** Mirrors `files::Outcome`: what a sync round did to documents and the library. */
export interface FilesSynced {
  docIds: string[];
  library: boolean;
  conflicts: Conflict[];
  partial: string[];
  /** Items that stay out of sync for now (too big, unreadable). */
  held: string[];
  /** From other devices, not applied here: `[name, why]`. */
  failed: [string, string][];
  /** Deleted skills whose folders were kept for files that never sync. */
  kept: string[];
}

export interface Notice {
  conflicts: Conflict[];
  held: string[];
  failed: [string, string][];
  kept: string[];
}

interface SyncStore {
  status: SyncStatus | null;
  /** Conflicts and skipped items to tell the user about (until dismissed). */
  notice: Notice | null;
  /** Skills with files that stay on this device (binary, large or hidden). */
  partial: string[];
  dismissNotice: () => void;
  /** Shows conflicts found outside a sync round (the open document). */
  addConflicts: (conflicts: Conflict[]) => void;
  init: () => Promise<void>;
  syncNow: () => void;
  setEnabled: (enabled: boolean) => Promise<void>;
}

export const useSyncStore = create<SyncStore>((set, get) => {
  let listening = false;
  /** Held and failed items already shown (they come every round). */
  const told = new Set<string>();
  const add = (more: Partial<Notice>) => {
    const prev = get().notice;
    set({
      notice: {
        conflicts: [...(prev?.conflicts ?? []), ...(more.conflicts ?? [])],
        held: [...(prev?.held ?? []), ...(more.held ?? [])],
        failed: [...(prev?.failed ?? []), ...(more.failed ?? [])],
        kept: [...(prev?.kept ?? []), ...(more.kept ?? [])],
      },
    });
  };
  return {
    status: null,
    notice: null,
    partial: [],
    dismissNotice: () => set({ notice: null }),
    addConflicts: (conflicts) => add({ conflicts }),

    init: async () => {
      if (!isTauri()) return;
      if (!listening) {
        listening = true;
        void listen<SyncStatus>("sync-status", (event) => set({ status: event.payload }));
        void listen<FilesSynced>("files-synced", ({ payload }) => {
          // Lazily: the sidebar bundle doesn't carry the composer.
          if (payload.docIds.length) {
            void import("./composer").then((m) =>
              m.useComposerStore.getState().reloadFromDisk(payload.docIds),
            );
          }
          if (payload.library) void useLibraryStore.getState().load(true);
          set({ partial: payload.partial });
          // Held and failed items come every round: tell about each once.
          const held = payload.held.filter((n) => !told.has(n));
          for (const n of held) told.add(n);
          const failed = payload.failed.filter(([n, why]) => !told.has(`${n}\n${why}`));
          for (const [n, why] of failed) told.add(`${n}\n${why}`);
          if (payload.conflicts.length || held.length || failed.length || payload.kept.length) {
            add({ conflicts: payload.conflicts, held, failed, kept: payload.kept });
          }
        });
        // Coming back to the app is a good moment to fetch other devices' edits.
        window.addEventListener("focus", () => void invoke("sync_now").catch(() => {}));
      }
      set({ status: await invoke<SyncStatus>("sync_status") });
    },

    syncNow: () => {
      if (isTauri()) void invoke("sync_now").catch(() => {});
    },

    setEnabled: async (enabled) => {
      if (!isTauri()) return;
      await invoke("sync_set_enabled", { enabled });
    },
  };
});
