import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

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

interface SyncStore {
  status: SyncStatus | null;
  init: () => Promise<void>;
  syncNow: () => void;
  setEnabled: (enabled: boolean) => Promise<void>;
}

export const useSyncStore = create<SyncStore>((set) => {
  let listening = false;
  return {
    status: null,

    init: async () => {
      if (!isTauri()) return;
      if (!listening) {
        listening = true;
        void listen<SyncStatus>("sync-status", (event) => set({ status: event.payload }));
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
