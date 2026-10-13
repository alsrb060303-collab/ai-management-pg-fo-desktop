import { isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";

/**
 * App updates from GitHub Releases (`plugins.updater` in tauri.conf.json).
 * The release workflow publishes a signed installer and `latest.json` for
 * every new version merged to main.
 */
type Status = "idle" | "checking" | "none" | "available" | "downloading" | "error";

interface UpdaterState {
  status: Status;
  /** The version on offer. */
  version: string | null;
  notes: string | null;
  /** Download progress, 0 to 1, when the size is known. */
  progress: number | null;
  error: string | null;
  /** The user closed the notice for this version. */
  dismissed: string | null;
  check: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
  dismiss: () => void;
}

let update: Update | null = null;
/** Checks again this often while the app stays open. */
const INTERVAL_MS = 6 * 60 * 60 * 1000;

export const useUpdaterStore = create<UpdaterState>((set, get) => ({
  status: "idle",
  version: null,
  notes: null,
  progress: null,
  error: null,
  dismissed: null,

  check: async (manual = false) => {
    const { status } = get();
    if (!isTauri() || status === "checking" || status === "downloading") return;
    set({ status: "checking", error: null });
    try {
      const found = await check();
      update = found;
      if (found) {
        set({
          status: "available",
          version: found.version,
          notes: found.body ?? null,
          ...(manual ? { dismissed: null } : {}),
        });
      } else {
        set({ status: "none", version: null, notes: null });
      }
    } catch (e) {
      // A background check that fails (offline, no release yet) stays quiet.
      set({ status: manual ? "error" : "idle", error: manual ? String(e) : null });
    }
  },

  install: async () => {
    if (!update || get().status === "downloading") return;
    let total = 0;
    let got = 0;
    set({ status: "downloading", progress: null, error: null });
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        else if (event.event === "Progress") {
          got += event.data.chunkLength;
          if (total) set({ progress: Math.min(got / total, 1) });
        }
      });
      // On Windows the installer closes the app itself; elsewhere restart it.
      await relaunch();
    } catch (e) {
      set({ status: "error", error: String(e) });
    }
  },

  dismiss: () => set({ dismissed: get().version }),
}));

let started = false;

/** Checks shortly after launch, then every few hours. */
export function startUpdateChecks() {
  if (started || !isTauri()) return;
  started = true;
  setTimeout(() => void useUpdaterStore.getState().check(), 5000);
  setInterval(() => void useUpdaterStore.getState().check(), INTERVAL_MS);
}
