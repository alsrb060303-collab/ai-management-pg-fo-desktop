import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";

export type ThemePref = "system" | "light" | "dark";
type Resolved = "light" | "dark";

// Both windows share this origin's localStorage, so a change in one reaches the
// other through the `storage` event.
const KEY = "theme";
const dark = window.matchMedia("(prefers-color-scheme: dark)");

function readPref(): ThemePref {
  try {
    const value = localStorage.getItem(KEY);
    if (value === "light" || value === "dark" || value === "system") return value;
  } catch {
    // Storage can be unavailable; fall back to the OS setting.
  }
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
      try {
        localStorage.setItem(KEY, next);
      } catch {
        // Still applies for this session.
      }
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
  window.addEventListener("storage", (event) => {
    if (event.key !== KEY) return;
    const pref = readPref();
    useThemeStore.setState({ pref, resolved: apply(pref) });
  });
}
