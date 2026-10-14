import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import { getPref, onPrefChanged, setPref } from "./prefs";

export type ThemePref = "system" | "light" | "dark";
type Resolved = "light" | "dark";

// Kept in prefs: both windows (and synced devices) follow a change through
// `prefs-changed`.
const KEY = "theme";
const dark = window.matchMedia("(prefers-color-scheme: dark)");

function readPref(): ThemePref {
  const value = getPref(KEY);
  if (value === "light" || value === "dark" || value === "system") return value;
  return "system";
}

function resolve(pref: ThemePref): Resolved {
  if (pref === "system") return dark.matches ? "dark" : "light";
  return pref;
}

function apply(pref: ThemePref) {
  const resolved = resolve(pref);
  document.documentElement.dataset.theme = resolved;
  // Native title bar and context menus follow too. `null` means follow the OS.
  if (isTauri()) {
    getCurrentWindow()
      .setTheme(pref === "system" ? null : pref)
      .catch(() => {});
  }
  return resolved;
}

interface ThemeState {
  pref: ThemePref;
  resolved: Resolved;
  setPref: (pref: ThemePref) => void;
}

export const useThemeStore = create<ThemeState>((set) => {
  const pref = readPref();
  return {
    pref,
    resolved: resolve(pref),
    setPref: (next) => {
      setPref(KEY, next);
      set({ pref: next, resolved: apply(next) });
    },
  };
});

/** Applies the saved theme before the first render and keeps it in sync. */
export function initTheme() {
  apply(useThemeStore.getState().pref);
  dark.addEventListener("change", () => {
    const { pref } = useThemeStore.getState();
    if (pref === "system") useThemeStore.setState({ resolved: apply(pref) });
  });
  onPrefChanged(KEY, () => {
    const pref = readPref();
    useThemeStore.setState({ pref, resolved: apply(pref) });
  });
}
