import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { useLibraryStore } from "./library";

// Mirrors src-tauri/src/sync.
export type SyncState =
  | "notConfigured"
  | "off"
  | "needsGitHub"
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

/** Mirrors `files::Outcome`: what a sync round did to documents and the library. */
export interface FilesSynced {
  docs: boolean;
  library: boolean;
  conflicts: {
    kind: "doc" | "skill" | "agent";
    name: string;
    copy: string | null;
    revived: boolean;
  }[];
  partial: string[];
  tooBig: string[];
}

interface SyncStore {
  status: SyncStatus | null;
  /** Conflicts and skipped items to tell the user about (until dismissed). */
  notice: Pick<FilesSynced, "conflicts" | "tooBig"> | null;
  /** Skills with files that stay on this device (binary, large or hidden). */
  partial: string[];
  dismissNotice: () => void;
  init: () => Promise<void>;
  syncNow: () => void;
  setEnabled: (enabled: boolean) => Promise<void>;
}

export const useSyncStore = create<SyncStore>((set, get) => {
  let listening = false;
  return {
    status: null,
    notice: null,
    partial: [],
    dismissNotice: () => set({ notice: null }),

    init: async () => {
      if (!isTauri()) return;
      if (!listening) {
        listening = true;
        void listen<SyncStatus>("sync-status", (event) => set({ status: event.payload }));
        void listen<FilesSynced>("files-synced", ({ payload }) => {
          // Lazily: the sidebar bundle doesn't carry the composer.
          if (payload.docs) {
            void import("./composer").then((m) => m.useComposerStore.getState().reloadFromDisk());
          }
          if (payload.library) void useLibraryStore.getState().load(true);
          set({ partial: payload.partial });
          if (payload.conflicts.length || payload.tooBig.length) {
            const prev = get().notice;
            set({
              notice: {
                conflicts: [...(prev?.conflicts ?? []), ...payload.conflicts],
                tooBig: [...new Set([...(prev?.tooBig ?? []), ...payload.tooBig])],
              },
            });
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
