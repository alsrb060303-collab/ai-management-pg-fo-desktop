import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

// Mirrors `DockStatus` in src-tauri/src/dock/mod.rs.
export type TargetApp = "windowsTerminal" | "console" | "vsCode" | "claudeDesktop" | "codexDesktop";
export type DockMode = "docked" | "edge" | "floating";
export type DockSide = "right" | "left" | "inside";
/** auto: follow the window in front · locked: stay with one window · off: no docking */
export type Magnet = "auto" | "locked" | "off";

export interface DockStatus {
  magnet: Magnet;
  pinned: boolean;
  visible: boolean;
  mode: DockMode;
  target: TargetApp | null;
  side: DockSide | null;
}

interface DockState {
  status: DockStatus | null;
  /** auto → locked → off → auto */
  cycleMagnet: () => Promise<void>;
  setPinned: (pinned: boolean) => Promise<void>;
  hide: () => Promise<void>;
}

export const useDockStore = create<DockState>(() => ({
  status: null,
  cycleMagnet: () => call("dock_cycle_magnet"),
  setPinned: (pinned) => call("dock_set_pinned", { pinned }),
  hide: () => call("sidebar_hide"),
}));

async function call(command: string, args?: Record<string, unknown>) {
  if (isTauri()) await invoke(command, args);
}

/** Loads the current status and keeps the store in sync with backend events. */
export async function subscribeDockStatus(): Promise<() => void> {
  if (!isTauri()) return () => {};
  const unlisten = await listen<DockStatus>("dock-status", (event) => {
    useDockStore.setState({ status: event.payload });
  });
  const status = await invoke<DockStatus>("dock_status");
  useDockStore.setState((state) => ({ status: state.status ?? status }));
  return unlisten;
}
