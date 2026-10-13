import { isTauri } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { create } from "zustand";
import { type MessageKey, t } from "../i18n";
import type { RepoView } from "./github";

const KEY = "notify.github";

function loadOn(): boolean {
  try {
    return localStorage.getItem(KEY) !== "0";
  } catch {
    return true;
  }
}

interface NotifyState {
  /** Desktop notifications when CI finishes or a PR is merged or closed. */
  on: boolean;
  setOn: (on: boolean) => void;
}

export const useNotifyStore = create<NotifyState>((set) => ({
  on: loadOn(),
  setOn: (on) => {
    try {
      localStorage.setItem(KEY, on ? "1" : "0");
    } catch {
      // Only a convenience.
    }
    set({ on });
    if (on) void allowed();
  },
}));

let permission: Promise<boolean> | null = null;

/** Asks the OS once per run; a refusal just means no notifications. */
function allowed(): Promise<boolean> {
  permission ??= (async () => {
    if (!isTauri()) return false;
    try {
      if (await isPermissionGranted()) return true;
      return (await requestPermission()) === "granted";
    } catch {
      return false;
    }
  })();
  return permission;
}

type Kind = "merged" | "closed" | "ciFailed" | "ciPassed";
interface Change {
  kind: Kind;
  vars: Record<string, string | number>;
}

/**
 * What changed between two loads of the same project's GitHub view that is
 * worth a notification: the branch's PR merged or closed, or its checks
 * finished. Pure, for testing.
 */
export function githubChanges(before: RepoView, after: RepoView): Change[] {
  const out: Change[] = [];
  const a = before.current;
  const b = after.current;
  // Only the same pull request (a branch switch is not news).
  if (!a || !b || a.number !== b.number) return out;
  const vars = { n: b.number, title: b.title };
  if (a.state === "open" && b.state === "merged") out.push({ kind: "merged", vars });
  else if (a.state === "open" && b.state === "closed") out.push({ kind: "closed", vars });
  const was = before.checks;
  const now = after.checks;
  if (was && now && was.pending > 0 && now.pending === 0) {
    if (now.failed > 0) out.push({ kind: "ciFailed", vars: { ...vars, failed: now.failed } });
    else if (now.passed > 0) out.push({ kind: "ciPassed", vars });
  }
  return out;
}

/** Sends a notification for each change, when turned on and allowed. */
export async function notifyGitHub(before: RepoView, after: RepoView) {
  if (!useNotifyStore.getState().on) return;
  const changes = githubChanges(before, after);
  if (!changes.length || !(await allowed())) return;
  for (const c of changes) {
    sendNotification({
      title: t(`notify.${c.kind}.title` as MessageKey, c.vars),
      body: t(`notify.${c.kind}.body` as MessageKey, c.vars),
    });
  }
}
