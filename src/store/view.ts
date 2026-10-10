import { invoke, isTauri } from "@tauri-apps/api/core";

// Mirrors `ViewMode` in src-tauri/src/shell.rs.
export type ViewMode = "main" | "sidebar";

/** Switches between the main window and the docked sidebar (only one is shown). */
export async function switchView(mode: ViewMode) {
  if (isTauri()) await invoke("view_switch", { mode });
}
