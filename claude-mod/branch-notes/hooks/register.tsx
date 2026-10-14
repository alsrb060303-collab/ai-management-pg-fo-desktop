import type { EngineInterface, Register } from "claude-code";
import { atom, read, update } from "claude-code";
import {
  applyLocally,
  bandText,
  contextText,
  parseExport,
  parseTodo,
  type Request,
  repoKeyFor,
  requestName,
  STALE_MS,
  todoList,
  type View,
  words,
} from "./core";

const APP_ID = "io.github.alsrb060303collab.aimanagement";
const PANE = "branch-notes";
/** How often the app's export and the checked-out branch are read again. */
const REFRESH_MS = 5000;

const view = atom({ plugin: "branch-notes", key: "view" } as const, null);
const isHidden = atom({ plugin: "branch-notes", key: "isHidden" } as const, false);

/** `<app data>/bridge`, where the app writes `state.json` and reads `inbox/`. */
async function bridgeDir($: EngineInterface): Promise<string | null> {
  const override = await $.env.get("AIM_BRIDGE_DIR");
  if (override) return override;
  const appData = await $.env.get("APPDATA");
  if (appData) return `${appData}\\${APP_ID}\\bridge`;
  const home = await $.env.get("HOME");
  if (!home) return null;
  const mac = `${home}/Library/Application Support/${APP_ID}/bridge`;
  if (await $.fs.exists(mac)) return mac;
  const xdg = (await $.env.get("XDG_DATA_HOME")) || `${home}/.local/share`;
  return `${xdg}/${APP_ID}/bridge`;
}

const sep = (dir: string) => (dir.includes("\\") ? "\\" : "/");

async function git($: EngineInterface, args: string[]): Promise<string | null> {
  try {
    const out = await $.process.run(["git", ...args]);
    return out.exitCode === 0 ? out.stdout.trim() || null : null;
  } catch {
    return null;
  }
}

/** Reads the app's export and finds this session's repository and branch in it. */
async function load($: EngineInterface): Promise<View | null> {
  const dir = await bridgeDir($);
  if (!dir) return null;
  let text: string;
  try {
    text = (await $.fs.read(`${dir}${sep(dir)}state.json`)) as string;
  } catch {
    return null;
  }
  const exp = parseExport(text);
  if (!exp) return null;
  const now = await $.clock.now();
  const toplevel = await git($, ["rev-parse", "--show-toplevel"]);
  if (!toplevel) {
    return {
      appRunning: now - exp.heartbeat < STALE_MS,
      locale: exp.locale,
      repoKey: null,
      branch: "",
      data: null,
    };
  }
  const remote =
    (await git($, ["remote", "get-url", "origin"])) ??
    (await git($, ["config", "--get-regexp", "^remote\\..*\\.url$"]))?.split(/\s+/)[1] ??
    null;
  const common = await git($, ["rev-parse", "--path-format=absolute", "--git-common-dir"]);
  const main = common ? common.replace(/[\\/]\.git[\\/]?$/, "") : toplevel;
  const repoKey = repoKeyFor(exp, remote, main);
  const branch = (await git($, ["symbolic-ref", "--quiet", "--short", "HEAD"])) ?? "";
  return {
    appRunning: now - exp.heartbeat < STALE_MS,
    locale: exp.locale,
    repoKey,
    branch,
    data: (repoKey && exp.repos[repoKey]?.[branch]) || null,
  };
}

async function refresh($: EngineInterface) {
  const next = await load($);
  const prev = await read($, view);
  if (JSON.stringify(next) !== JSON.stringify(prev)) await update($, view, () => next);
}

/** Leaves a request for the app and shows it at once. */
async function send($: EngineInterface, request: Request): Promise<boolean> {
  const dir = await bridgeDir($);
  if (!dir) return false;
  const s = sep(dir);
  try {
    await $.fs.write(
      `${dir}${s}inbox${s}${requestName(await $.clock.now(), Math.random())}`,
      JSON.stringify(request),
    );
  } catch {
    return false;
  }
  await update($, view, (v) => (v ? applyLocally(v, request) : v));
  return true;
}

export const register: Register = (on) => {
  on("session.start", async ($, e, next) => {
    await $.command.register({
      name: "todo",
      description: "Branch to-dos (Encela): /todo, /todo <text>, /todo done <n>",
    });
    await $.command.register({
      name: "memo",
      description: "Branch note (Encela): /memo, /memo <text>",
    });
    await $.command.register({
      name: "notes",
      description: "Open the branch notes pane (Encela)",
    });
    await refresh($);
    $.clock.every(REFRESH_MS, () => void refresh($));
    return next(e);
  });

  // The branch can change between turns (git switch in the session).
  on("turn.complete", async ($, e, next) => {
    await refresh($);
    return next(e);
  });

  on("command.run", { command: "todo" }, async ($, e) => {
    await refresh($);
    const v = await read($, view);
    const w = words(v?.locale ?? "en");
    if (!v?.repoKey) return { text: v ? w.noRepo : w.noApp };
    const cmd = parseTodo(e.args);
    const tail = v.appRunning ? "" : `\n${w.noApp}`;
    switch (cmd.kind) {
      case "help":
        return { text: w.todoHelp };
      case "list":
        return { text: `${todoList(v)}\n\n${w.todoHelp}` };
      case "add":
        if (
          !(await send($, { op: "addTodo", repoKey: v.repoKey, branch: v.branch, text: cmd.text }))
        ) {
          return { text: w.noApp };
        }
        return { text: w.added(cmd.text) + tail };
      default: {
        const todo = v.data?.todos[cmd.index - 1];
        if (!todo) return { text: w.noSuch(cmd.index) };
        const request: Request =
          cmd.kind === "rm"
            ? { op: "deleteTodo", id: todo.id }
            : { op: "setDone", id: todo.id, done: cmd.kind === "done" };
        if (!(await send($, request))) return { text: w.noApp };
        const said = cmd.kind === "rm" ? w.removed : cmd.kind === "done" ? w.done : w.undone;
        return { text: said(todo.text) + tail };
      }
    }
  });

  on("command.run", { command: "memo" }, async ($, e) => {
    await refresh($);
    const v = await read($, view);
    const w = words(v?.locale ?? "en");
    if (!v?.repoKey) return { text: v ? w.noRepo : w.noApp };
    const text = e.args.trim();
    if (!text) return { text: v.data?.note.trim() ? v.data.note : `${w.empty}\n${w.memoHelp}` };
    if (!(await send($, { op: "appendNote", repoKey: v.repoKey, branch: v.branch, text }))) {
      return { text: w.noApp };
    }
    return { text: w.noted + (v.appRunning ? "" : `\n${w.noApp}`) };
  });

  on("command.run", { command: "notes" }, async ($) => {
    const v = await read($, view);
    await $.ui.open({ id: PANE, title: words(v?.locale ?? "en").paneTitle });
    await update($, isHidden, () => false);
    return {};
  });

  // One line above the prompt: the branch, to-dos left, the next one, the PR.
  on("ui.render", { component: "AbovePrompt" }, async ($, e, next) => {
    const v = await read($, view);
    const text = v ? bandText(v) : null;
    if (!v || !text || e.props.hasSurvey || (await read($, isHidden))) return next(e);
    const { Box, Button, Text } = $.ui.resolve(e);
    return (
      <Box>
        <Text dimColor wrap="truncate">
          {text}{" "}
        </Text>
        <Button
          key="open"
          label={words(v.locale).paneTitle}
          dimColor
          onPress={() => void $.ui.open({ id: PANE, title: words(v.locale).paneTitle })}
        />
        <Button key="hide" label="×" dimColor onPress={() => update($, isHidden, () => true)} />
      </Box>
    );
  });

  // The pane: the note, and the to-dos to tick.
  on("ui.render", { component: "Pane", requestId: PANE }, async ($, e) => {
    const { Box, Button, Markdown, Text } = $.ui.resolve(e);
    const v = await read($, view);
    const w = words(v?.locale ?? "en");
    if (!v?.repoKey) return <Text dimColor>{v ? w.noRepo : w.noApp}</Text>;
    const d = v.data;
    return (
      <Box flexDirection="column">
        <Text bold>⎇ {v.branch || "-"}</Text>
        {!v.appRunning && <Text dimColor>{w.noApp}</Text>}
        {!d && <Text dimColor>{w.empty}</Text>}
        {d?.note.trim() ? (
          <Box flexDirection="column">
            <Text bold>{w.noteTitle}</Text>
            <Markdown key="note" text={d.note} />
          </Box>
        ) : null}
        {d?.todos.length ? (
          <Box flexDirection="column">
            <Text bold>{w.todosTitle}</Text>
            {d.todos.map((t, i) => (
              <Button
                key={`todo-${t.id}`}
                plain
                dimColor={t.done}
                onPress={() => void send($, { op: "setDone", id: t.id, done: !t.done })}
              >
                {`${i + 1}. ${t.done ? "☑" : "☐"} ${t.text}`}
              </Button>
            ))}
          </Box>
        ) : null}
        <Text dimColor>{w.todoHelp}</Text>
      </Box>
    );
  });

  // Claude reads the branch's note and to-dos with the system prompt.
  on("prompt.compose", async ($, e, next) => {
    const result = await next(e);
    const v = await read($, view);
    const text = v ? contextText(v) : null;
    if (!text) return result;
    return {
      ...result,
      sections: [
        ...result.sections,
        { id: "branch-notes:context", text, scope: "session" as const },
      ],
    };
  });
};
