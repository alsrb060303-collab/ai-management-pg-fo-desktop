import { describe, expect, test } from "claude-code/testing";
import {
  applyLocally,
  bandText,
  contextText,
  normalizeRemote,
  parseExport,
  parseTodo,
  repoKeyFor,
  requestName,
  type View,
} from "./core";

const view: View = {
  appRunning: true,
  locale: "en",
  repoKey: "github.com/me/app",
  branch: "feat/login",
  data: {
    note: "Use the new auth API",
    todos: [
      { id: "a", text: "Write tests", done: true },
      { id: "b", text: "Wire the form", done: false },
    ],
    github: {
      prNumber: 12,
      prTitle: "Login",
      prState: "open",
      prUrl: "u",
      passed: 3,
      failed: 1,
      pending: 0,
    },
  },
};

describe("keys match the app's", () => {
  test("remotes normalize like src-tauri/src/memo/repo.rs", () => {
    expect(normalizeRemote("https://github.com/Owner/Repo.git")).toBe("github.com/owner/repo");
    expect(normalizeRemote("git@github.com:owner/repo")).toBe("github.com/owner/repo");
    expect(normalizeRemote("ssh://git@github.com:22/owner/repo/")).toBe("github.com/owner/repo");
    expect(normalizeRemote("https://user:tok@gitlab.com/a/b")).toBe("gitlab.com/a/b");
    expect(normalizeRemote("C:\\repos\\x")).toBe("file:C:/repos/x");
  });

  test("a repository without a remote is found by its folder", () => {
    const exp = parseExport(
      JSON.stringify({
        version: 1,
        heartbeat: 0,
        locale: "en",
        paths: { "path:\\\\?\\C:\\Code\\App": "path:C:\\Code\\App" },
        repos: {},
      }),
    );
    expect(exp).not.toBe(null);
    if (!exp) return;
    expect(repoKeyFor(exp, null, "C:/code/app")).toBe("path:C:\\Code\\App");
    expect(repoKeyFor(exp, "git@github.com:o/r.git", null)).toBe("github.com/o/r");
    expect(parseExport("{")).toBe(null);
    expect(parseExport('{"version":2,"repos":{}}')).toBe(null);
  });
});

describe("what the mod shows", () => {
  test("the band sums up the branch", () => {
    expect(bandText(view)).toBe(
      "⎇ feat/login · 1/2 to-dos left · Next: Wire the form · PR #12 open ✗1 ✓3",
    );
    expect(bandText({ ...view, data: { note: "", todos: [] } })).toBe(null);
    expect(bandText({ ...view, data: null })).toBe(null);
  });

  test("Claude gets the note and the numbered to-dos", () => {
    const text = contextText(view) ?? "";
    expect(text).toContain("Use the new auth API");
    expect(text).toContain("1. [x] Write tests");
    expect(text).toContain("2. [ ] Wire the form");
    expect((contextText(view, 50) ?? "").length).toBe(50);
  });
});

describe("commands", () => {
  test("/todo arguments", () => {
    expect(parseTodo("")).toEqual({ kind: "list" });
    expect(parseTodo("done 2")).toEqual({ kind: "done", index: 2 });
    expect(parseTodo("완료 1")).toEqual({ kind: "done", index: 1 });
    expect(parseTodo("rm 3")).toEqual({ kind: "rm", index: 3 });
    expect(parseTodo("undo 1")).toEqual({ kind: "undo", index: 1 });
    expect(parseTodo("done with the API")).toEqual({ kind: "add", text: "done with the API" });
  });

  test("requests show at once and sort in order", () => {
    const after = applyLocally(view, { op: "setDone", id: "b", done: true });
    expect(after.data?.todos.every((t) => t.done)).toBe(true);
    const noted = applyLocally(view, {
      op: "appendNote",
      repoKey: "k",
      branch: "b",
      text: "More",
    });
    expect(noted.data?.note).toBe("Use the new auth API\nMore");
    expect(requestName(5, 0.5) < requestName(40, 0.1)).toBe(true);
  });
});
