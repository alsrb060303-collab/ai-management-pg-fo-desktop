// The mod's logic without the engine: reading the app's export, finding this
// session's repository in it, wording, and parsing commands. Kept in sync
// with src-tauri/src/bridge.rs (the export and the requests) and
// src-tauri/src/memo/repo.rs (`normalize_remote`).

import type { BranchData, View } from "../types";

export type { BranchData, GitHubSummary, Todo, View } from "../types";

export type Export = {
  version: number;
  heartbeat: number;
  locale: "ko" | "en";
  paths: Record<string, string>;
  repos: Record<string, Record<string, BranchData>>;
};

/** The heartbeat is rewritten every 30 seconds while the app runs. */
export const STALE_MS = 2 * 60 * 1000;

/** `https://github.com/Owner/Repo.git` → `github.com/owner/repo`, as the app does. */
export function normalizeRemote(raw: string): string {
  let url = raw.trim().replace(/\/+$/, "");
  if (url.endsWith(".git")) url = url.slice(0, -4);
  let rest: string;
  const scheme = url.indexOf("://");
  if (scheme >= 0) {
    let after = url.slice(scheme + 3);
    const at = after.lastIndexOf("@");
    if (at >= 0) after = after.slice(at + 1);
    const slash = after.indexOf("/");
    const host = (slash >= 0 ? after.slice(0, slash) : after).split(":")[0];
    rest = `${host}/${slash >= 0 ? after.slice(slash + 1) : ""}`;
  } else {
    const colon = url.indexOf(":");
    const userHost = colon > 1 ? url.slice(0, colon) : "";
    if (colon > 1 && !/[/\\]/.test(userHost)) {
      const at = userHost.lastIndexOf("@");
      const host = at >= 0 ? userHost.slice(at + 1) : userHost;
      rest = `${host}/${url.slice(colon + 1).replace(/^\/+/, "")}`;
    } else {
      return `file:${url.replace(/\\/g, "/")}`;
    }
  }
  return rest.replace(/\/+$/, "").toLowerCase();
}

/** Paths compared the way Windows and the app's canonical paths can differ. */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) =>
    p
      .replace(/^path:/, "")
      .replace(/\\/g, "/")
      .replace(/^\/\/\?\//, "")
      .replace(/\/+$/, "")
      .toLowerCase();
  return norm(a) === norm(b);
}

/**
 * The app's key for a repository: its normalized remote, or, without one,
 * whatever the app filed the main worktree's folder under.
 */
export function repoKeyFor(
  exp: Export,
  remote: string | null,
  mainWorktree: string | null,
): string | null {
  if (remote) return normalizeRemote(remote);
  if (!mainWorktree) return null;
  for (const [pathKey, key] of Object.entries(exp.paths)) {
    if (samePath(pathKey, mainWorktree)) return key;
  }
  return `path:${mainWorktree}`;
}

export function parseExport(text: string): Export | null {
  try {
    const value = JSON.parse(text);
    if (value?.version !== 1 || typeof value.repos !== "object") return null;
    return value as Export;
  } catch {
    return null;
  }
}

const WORDS = {
  ko: {
    left: (open: number, total: number) => `할 일 ${open}/${total} 남음`,
    allDone: "할 일 모두 완료",
    next: "다음",
    noApp: "AI Management 앱이 꺼져 있어요. 켜면 반영돼요.",
    added: (text: string) => `할 일 추가: ${text}`,
    noted: "메모에 추가했어요.",
    done: (text: string) => `완료: ${text}`,
    undone: (text: string) => `다시 열림: ${text}`,
    removed: (text: string) => `삭제: ${text}`,
    noSuch: (n: number) => `${n}번 할 일이 없어요.`,
    noRepo: "이 폴더는 git 저장소가 아니거나 아직 앱에서 연 적이 없어요.",
    empty: "이 브랜치에는 메모와 할 일이 없어요.",
    todoHelp:
      "/todo 할 일 추가 · /todo done 2 완료 · /todo undo 2 다시 열기 · /todo rm 2 삭제 · /todo 목록",
    memoHelp: "/memo 내용 → 메모에 한 줄 추가 · /memo → 메모 보기",
    noteTitle: "메모",
    todosTitle: "할 일",
    paneTitle: "브랜치 메모",
    pr: "PR",
    context: (branch: string) =>
      `사용자가 이 저장소의 '${branch}' 브랜치에 남긴 메모와 할 일이에요 (AI Management 앱). 작업 방향을 정할 때 참고하고, 끝낸 일은 사용자에게 /todo done 번호 로 체크하라고 알려 주세요.`,
  },
  en: {
    left: (open: number, total: number) => `${open}/${total} to-dos left`,
    allDone: "All to-dos done",
    next: "Next",
    noApp: "The AI Management app isn't running. It applies this when it starts.",
    added: (text: string) => `Added to-do: ${text}`,
    noted: "Added to the note.",
    done: (text: string) => `Done: ${text}`,
    undone: (text: string) => `Reopened: ${text}`,
    removed: (text: string) => `Removed: ${text}`,
    noSuch: (n: number) => `There's no to-do #${n}.`,
    noRepo: "This folder isn't a git repository, or the app hasn't opened it yet.",
    empty: "No note or to-dos on this branch.",
    todoHelp: "/todo text adds · /todo done 2 · /todo undo 2 · /todo rm 2 · /todo lists them",
    memoHelp: "/memo text adds a line to the note · /memo shows it",
    noteTitle: "Note",
    todosTitle: "To-dos",
    paneTitle: "Branch notes",
    pr: "PR",
    context: (branch: string) =>
      `The user's note and to-dos for the '${branch}' branch of this repository (from the AI Management app). Use them to steer the work, and tell the user to tick finished items with /todo done <number>.`,
  },
};

export const words = (locale: "ko" | "en") => WORDS[locale] ?? WORDS.en;

/** The band above the prompt, or `null` when there's nothing to show. */
export function bandText(view: View): string | null {
  const d = view.data;
  if (!d) return null;
  const w = words(view.locale);
  const parts: string[] = [`⎇ ${view.branch || "-"}`];
  const open = d.todos.filter((t) => !t.done);
  if (d.todos.length) {
    parts.push(open.length ? w.left(open.length, d.todos.length) : w.allDone);
    if (open[0]) parts.push(`${w.next}: ${open[0].text}`);
  }
  const g = d.github;
  if (g?.prNumber) {
    let ci = "";
    if (g.failed) ci += ` ✗${g.failed}`;
    if (g.pending) ci += ` …${g.pending}`;
    if (g.passed) ci += ` ✓${g.passed}`;
    parts.push(`${w.pr} #${g.prNumber} ${g.prState ?? ""}${ci}`.trim());
  }
  if (parts.length === 1 && !d.note.trim()) return null;
  return parts.join(" · ");
}

/** The section added to the system prompt, or `null` when there's nothing. */
export function contextText(view: View, maxChars = 2000): string | null {
  const d = view.data;
  if (!d || (!d.note.trim() && !d.todos.length)) return null;
  const w = words(view.locale);
  const lines = [w.context(view.branch)];
  if (d.note.trim()) lines.push("", `## ${w.noteTitle}`, d.note.trim());
  if (d.todos.length) {
    lines.push("", `## ${w.todosTitle}`);
    d.todos.forEach((t, i) => {
      lines.push(`${i + 1}. [${t.done ? "x" : " "}] ${t.text}`);
    });
  }
  const text = lines.join("\n");
  return text.length > maxChars ? `${text.slice(0, maxChars - 1)}…` : text;
}

/** The numbered list `/todo` prints. */
export function todoList(view: View): string {
  const d = view.data;
  const w = words(view.locale);
  if (!d?.todos.length) return w.empty;
  return d.todos.map((t, i) => `${i + 1}. ${t.done ? "☑" : "☐"} ${t.text}`).join("\n");
}

export type TodoCommand =
  | { kind: "list" }
  | { kind: "help" }
  | { kind: "add"; text: string }
  | { kind: "done" | "undo" | "rm"; index: number };

export function parseTodo(args: string): TodoCommand {
  const text = args.trim();
  if (!text) return { kind: "list" };
  if (text === "help" || text === "?") return { kind: "help" };
  const m = /^(done|undo|rm|완료|취소|삭제)\s+(\d+)$/i.exec(text);
  if (m) {
    const verb = (m[1] ?? "").toLowerCase();
    const kind =
      verb === "done" || verb === "완료"
        ? "done"
        : verb === "undo" || verb === "취소"
          ? "undo"
          : "rm";
    return { kind, index: Number(m[2]) };
  }
  return { kind: "add", text };
}

/** A request file for the app's inbox (src-tauri/src/bridge.rs `Request`). */
export type Request =
  | { op: "addTodo"; repoKey: string; branch: string; text: string }
  | { op: "setDone"; id: string; done: boolean }
  | { op: "deleteTodo"; id: string }
  | { op: "appendNote"; repoKey: string; branch: string; text: string };

/** A file name that sorts in arrival order and doesn't collide. */
export function requestName(now: number, random: number): string {
  return `${String(now).padStart(15, "0")}-${Math.floor(random * 1e9)}.json`;
}

/** Applies a request to the view right away, before the app confirms it. */
export function applyLocally(view: View, request: Request): View {
  const d = view.data ?? { note: "", todos: [] };
  let next: BranchData = d;
  switch (request.op) {
    case "addTodo":
      next = {
        ...d,
        todos: [...d.todos, { id: `pending-${d.todos.length}`, text: request.text, done: false }],
      };
      break;
    case "setDone":
      next = {
        ...d,
        todos: d.todos.map((t) => (t.id === request.id ? { ...t, done: request.done } : t)),
      };
      break;
    case "deleteTodo":
      next = { ...d, todos: d.todos.filter((t) => t.id !== request.id) };
      break;
    case "appendNote":
      next = { ...d, note: d.note.trim() ? `${d.note.trimEnd()}\n${request.text}` : request.text };
      break;
  }
  return { ...view, data: next };
}
