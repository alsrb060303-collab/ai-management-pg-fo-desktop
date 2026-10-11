import { invoke, isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";
import * as common from "./messages/common";
import * as composer from "./messages/composer";
import * as docs from "./messages/docs";
import * as memo from "./messages/memo";
import * as settings from "./messages/settings";
import * as shell from "./messages/shell";
import * as work from "./messages/work";

/**
 * UI strings in Korean and English. Each area keeps its strings in
 * `messages/<area>.ts` as a `ko` object and an `en` object with the same keys
 * (the `Messages` type makes a missing English string a type error).
 */
export type Locale = "ko" | "en";
export type LocalePref = "system" | Locale;

const KO = {
  ...common.ko,
  ...shell.ko,
  ...settings.ko,
  ...composer.ko,
  ...docs.ko,
  ...work.ko,
  ...memo.ko,
};
export type MessageKey = keyof typeof KO;
const EN: Record<MessageKey, string> = {
  ...common.en,
  ...shell.en,
  ...settings.en,
  ...composer.en,
  ...docs.en,
  ...work.en,
  ...memo.en,
};
const MESSAGES: Record<Locale, Record<MessageKey, string>> = { ko: KO, en: EN };

export type Params = Record<string, string | number>;

// Both windows share localStorage; a change in one reaches the other through
// the `storage` event (see the listener below).
const KEY = "language";

function readPref(): LocalePref {
  try {
    const value = localStorage.getItem(KEY);
    if (value === "ko" || value === "en" || value === "system") return value;
  } catch {
    // Storage can be unavailable; follow the OS.
  }
  return "system";
}

/** Korean when the OS (WebView) language is Korean, English otherwise. */
function systemLocale(): Locale {
  const langs = navigator.languages?.length ? navigator.languages : [navigator.language];
  return langs[0]?.toLowerCase().startsWith("ko") ? "ko" : "en";
}

function resolve(pref: LocalePref): Locale {
  return pref === "system" ? systemLocale() : pref;
}

const appliedListeners = new Set<() => void>();

/**
 * Runs `listener` after a language change has reached the backend, so text
 * the backend made (errors, settings-map notes, compile warnings) can be
 * fetched again in the new language. Returns an unsubscribe function.
 */
export function onLocaleApplied(listener: () => void): () => void {
  appliedListeners.add(listener);
  return () => appliedListeners.delete(listener);
}

/** Sets the language in the document and the backend; resolves when both have it. */
async function apply(locale: Locale, notify: boolean): Promise<void> {
  document.documentElement.lang = locale;
  // Backend messages (errors, settings-map notes, tray menu) follow too.
  if (isTauri()) await invoke("set_locale", { locale }).catch(() => {});
  if (notify) for (const listener of appliedListeners) listener();
}

/**
 * Resolves once the backend has the startup language; the app renders after
 * it so the first backend calls already answer in that language.
 */
export let localeReady: Promise<void> = Promise.resolve();

interface LocaleState {
  pref: LocalePref;
  locale: Locale;
  setPref: (pref: LocalePref) => void;
}

export const useLocaleStore = create<LocaleState>((set) => {
  const pref = readPref();
  const locale = resolve(pref);
  localeReady = apply(locale, false);
  return {
    pref,
    locale,
    setPref: (next) => {
      try {
        localStorage.setItem(KEY, next);
      } catch {
        // Still applies for this session.
      }
      const locale = resolve(next);
      set({ pref: next, locale });
      void apply(locale, true);
    },
  };
});

window.addEventListener("storage", (event) => {
  if (event.key !== KEY) return;
  const pref = readPref();
  const locale = resolve(pref);
  useLocaleStore.setState({ pref, locale });
  void apply(locale, true);
});

function format(template: string, params?: Params): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

/** Translates outside React (stores, helpers): uses the current language. */
export function t(key: MessageKey, params?: Params): string {
  return format(MESSAGES[useLocaleStore.getState().locale][key], params);
}

/** Translates in components; re-renders when the language changes. */
export function useT(): (key: MessageKey, params?: Params) => string {
  const locale = useLocaleStore((s) => s.locale);
  return (key, params) => format(MESSAGES[locale][key], params);
}

export function useLocale(): Locale {
  return useLocaleStore((s) => s.locale);
}

/** BCP 47 tag for `Intl` and `toLocaleString`. */
export function intlLocale(locale: Locale = useLocaleStore.getState().locale): string {
  return locale === "ko" ? "ko-KR" : "en-US";
}

/** "3분 전" / "3 min ago", falling back to the date after a month. */
export function timeAgo(epochSeconds: number, locale: Locale = useLocaleStore.getState().locale) {
  if (!epochSeconds) return "";
  const diff = Date.now() / 1000 - epochSeconds;
  const rtf = new Intl.RelativeTimeFormat(intlLocale(locale), { numeric: "auto", style: "short" });
  if (diff < 60) return rtf.format(0, "second");
  if (diff < 3600) return rtf.format(-Math.floor(diff / 60), "minute");
  if (diff < 86400) return rtf.format(-Math.floor(diff / 3600), "hour");
  if (diff < 86400 * 30) return rtf.format(-Math.floor(diff / 86400), "day");
  return new Date(epochSeconds * 1000).toLocaleDateString(intlLocale(locale));
}
