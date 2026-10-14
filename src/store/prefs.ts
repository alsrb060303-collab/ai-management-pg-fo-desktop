import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * App settings that must survive updates and follow the user between windows
 * and devices. The backend keeps them (`prefs_all`, `prefs_set`, event
 * `prefs-changed`); this is an in-memory cache loaded once at startup.
 * localStorage is only a fallback for when the backend is missing or slow, and
 * the source of the one-time migration of what older versions kept there.
 */

/** Keys older versions kept in localStorage: `[old name, pref name]`. */
const LEGACY_KEYS: [string, string][] = [
  ["theme", "theme"],
  ["language", "language"],
  ["fontSize", "fontSize"],
  ["notify.github", "notifyEnabled"],
  ["defaultTool", "defaultTool"],
  ["toolPrefs", "toolPrefs"],
  ["recentProjects", "recentProjects"],
  ["currentProject", "currentProject"],
  ["nav-collapsed", "nav-collapsed"],
  ["composer-preview", "composer-preview"],
  ["composer.largeBlockWarning.off", "composer.largeBlockWarning.off"],
  ["composer.write.mode", "composer.write.mode"],
  ["composer.write.repo", "composer.write.repo"],
  ["gitSetupDismissed", "gitSetupDismissed"],
  ["work.tab", "work.tab"],
  ["work.remote.tab", "work.remote.tab"],
  ["work.remoteRepo", "work.remoteRepo"],
  ["activity.view", "activity.view"],
];
/** Key families (panel widths are per panel). */
const LEGACY_PREFIXES = ["panel-width:"];

const LOAD_TIMEOUT_MS = 1000;

let cache: Record<string, string> = {};
const listeners = new Map<string, Set<(value: string | null) => void>>();

function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function lsSet(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // Storage can be unavailable; the cache still holds it for this run.
  }
}

function lsKeys(): string[] {
  try {
    return Array.from({ length: localStorage.length }, (_, i) => localStorage.key(i) ?? "");
  } catch {
    return [];
  }
}

function sendToBackend(key: string, value: string | null) {
  if (!isTauri()) return;
  invoke("prefs_set", { key, value }).catch(() => {});
}

/** Applies a change that came from outside; tells subscribers if it is news. */
function applyRemote(key: string, value: string | null) {
  if ((cache[key] ?? null) === value) return;
  if (value === null) delete cache[key];
  else cache[key] = value;
  lsSet(key, value);
  for (const cb of listeners.get(key) ?? []) cb(value);
}

async function fetchAll(): Promise<Record<string, string> | null> {
  if (!isTauri()) return null;
  const timeout = new Promise<null>((resolve) => setTimeout(() => resolve(null), LOAD_TIMEOUT_MS));
  try {
    const all = await Promise.race([invoke<Record<string, string>>("prefs_all"), timeout]);
    return all && typeof all === "object" ? all : null;
  } catch {
    return null;
  }
}

/** Copies what older versions kept in localStorage, for keys prefs lacks. */
function migrate(backendOk: boolean) {
  const pairs = [...LEGACY_KEYS];
  for (const key of lsKeys()) {
    if (LEGACY_PREFIXES.some((p) => key.startsWith(p))) pairs.push([key, key]);
  }
  for (const [old, key] of pairs) {
    if (key in cache) continue;
    // The old name first; the new name may hold a fallback copy from a run
    // without a backend.
    const value = lsGet(old) ?? (old === key ? null : lsGet(key));
    if (value === null) continue;
    cache[key] = value;
    if (backendOk) sendToBackend(key, value);
  }
}

let loaded: Promise<void> | null = null;

/**
 * Loads the settings once. Never rejects and never waits long, so a backend
 * failure cannot block the UI (the app then runs on localStorage values).
 */
export function loadPrefs(): Promise<void> {
  loaded ??= (async () => {
    const all = await fetchAll();
    cache = { ...(all ?? {}) };
    migrate(all !== null);
    if (isTauri()) {
      listen<{ key: string; value: string | null }>("prefs-changed", (event) => {
        applyRemote(event.payload.key, event.payload.value ?? null);
      }).catch(() => {});
    }
    // Without a backend, another window's change arrives as a storage event.
    window.addEventListener("storage", (event) => {
      if (event.key) applyRemote(event.key, event.newValue);
    });
  })();
  return loaded;
}

export function getPref(key: string): string | null {
  return cache[key] ?? null;
}

/** Saves a setting (`null` removes it). */
export function setPref(key: string, value: string | null) {
  if (value === null) delete cache[key];
  else cache[key] = value;
  lsSet(key, value);
  sendToBackend(key, value);
}

/** Runs `cb` when `key` changes from another window or device. Returns an unsubscribe function. */
export function onPrefChanged(key: string, cb: (value: string | null) => void): () => void {
  const subscribers = listeners.get(key) ?? new Set<(value: string | null) => void>();
  listeners.set(key, subscribers);
  subscribers.add(cb);
  return () => subscribers.delete(cb);
}
