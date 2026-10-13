import { ask } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { FileText, FolderOpen, Plus, RefreshCw, RotateCcw, Save, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { ResizeHandle, usePanelWidth } from "../../components/Resizable";
import { PageHeader } from "../../components/ui";
import { type MessageKey, useT } from "../../i18n";
import { useProjectStore } from "../../store/project";
import { type DocEntry, type DocKind, useProjectDocsStore } from "../../store/projectDocs";
import { TOOLS, type ToolId } from "../../tools";

/** How often the folder is re-read while the page is visible. */
const POLL_MS = 4000;

const KINDS: { kind: DocKind; label: MessageKey }[] = [
  { kind: "instructions", label: "docs.section.instructions" },
  { kind: "rule", label: "docs.section.rule" },
  { kind: "skill", label: "docs.section.skill" },
  { kind: "agent", label: "docs.section.agent" },
  { kind: "command", label: "docs.section.command" },
  { kind: "settings", label: "docs.section.settings" },
  { kind: "mcp", label: "docs.section.mcp" },
  { kind: "hook", label: "docs.section.hook" },
  { kind: "policy", label: "docs.section.policy" },
  { kind: "other", label: "files.kind.other" },
];

/** What "New document" can create, and where it goes. */
type NewKind =
  | "instructions"
  | "override"
  | "rule"
  | "skill"
  | "agent"
  | "command"
  | "settings"
  | "hook"
  | "policy";

/** The kinds each tool offers, in menu order. */
const NEW_KINDS: Record<ToolId, NewKind[]> = {
  claude: ["instructions", "rule", "skill", "agent", "command"],
  codex: ["instructions", "override", "skill", "agent", "settings", "hook", "policy"],
};

/** Kinds created in a folder (empty for the project root) rather than by name. */
const IN_FOLDER: NewKind[] = ["instructions", "override", "settings", "hook"];

function newKindLabel(tool: ToolId, kind: NewKind): MessageKey | string {
  switch (kind) {
    case "instructions":
      return TOOLS[tool].instructions.team;
    case "override":
      return "AGENTS.override.md";
    case "settings":
      return ".codex/config.toml";
    case "hook":
      return ".codex/hooks.json";
    default:
      return `docs.section.${kind}`;
  }
}

function newDoc(tool: ToolId, kind: NewKind, name: string): { rel: string; text: string } | null {
  const n = name
    .trim()
    .replace(/\\/g, "/")
    .replace(/^\/+|\/+$/g, "");
  const inFolder = (file: string) => (n ? `${n}/${file}` : file);
  if (tool === "codex") {
    switch (kind) {
      case "instructions":
        return { rel: inFolder("AGENTS.md"), text: "" };
      case "override":
        return { rel: inFolder("AGENTS.override.md"), text: "" };
      case "skill":
        return n
          ? {
              rel: `.agents/skills/${n}/SKILL.md`,
              text: `---\nname: ${n}\ndescription: \n---\n\n`,
            }
          : null;
      case "agent":
        return n
          ? {
              rel: `.codex/agents/${n}.toml`,
              text: `name = ${JSON.stringify(n)}\ndescription = ""\ndeveloper_instructions = """\n"""\n`,
            }
          : null;
      case "settings":
        return { rel: inFolder(".codex/config.toml"), text: "" };
      case "hook":
        return { rel: inFolder(".codex/hooks.json"), text: '{\n  "hooks": {}\n}\n' };
      case "policy":
        return n ? { rel: `.codex/rules/${n}.rules`, text: "" } : null;
      default:
        return null;
    }
  }
  switch (kind) {
    case "instructions":
      return { rel: inFolder("CLAUDE.md"), text: "" };
    case "rule":
      return n ? { rel: `.claude/rules/${n}.md`, text: "" } : null;
    case "skill":
      return n
        ? {
            rel: `.claude/skills/${n}/SKILL.md`,
            text: `---\nname: ${n}\ndescription: \n---\n\n`,
          }
        : null;
    case "agent":
      return n
        ? {
            rel: `.claude/agents/${n}.md`,
            text: `---\nname: ${n}\ndescription: \n---\n\n`,
          }
        : null;
    case "command":
      return n ? { rel: `.claude/commands/${n}.md`, text: "" } : null;
    default:
      return null;
  }
}

const joinPath = (project: string, rel: string) => {
  const sep = project.includes("\\") ? "\\" : "/";
  return `${project.replace(/[\\/]+$/, "")}${sep}${rel.split("/").join(sep)}`;
};

/** The project's AI tool documents, found automatically, edited in place. */
export function ProjectDocsPage() {
  const t = useT();
  const project = useProjectStore((s) => s.current);
  const tool = useProjectStore((s) => s.tool);
  // Per-tool data waits until a newly selected project's tool is known.
  const toolReady = useProjectStore((s) => s.toolReady || !s.current);
  const toolName = TOOLS[tool].name;
  const choose = useProjectStore((s) => s.choose);
  const { entries, loading, error, rel, load, refresh, dismissError } = useProjectDocsStore();
  const [creating, setCreating] = useState(false);
  const docsPanel = usePanelWidth("project-docs", 256, 180, 520);

  useEffect(() => {
    if (toolReady) void load(project, tool);
  }, [project, tool, toolReady, load]);

  // Pick up files the AI tool or another editor adds or changes.
  useEffect(() => {
    const tick = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    const timer = setInterval(tick, POLL_MS);
    window.addEventListener("focus", tick);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", tick);
    };
  }, [refresh]);

  return (
    <div className="mx-auto flex h-full max-w-6xl flex-col px-8 py-10">
      <PageHeader
        title={t("files.title")}
        description={t(tool === "codex" ? "files.descriptionCodex" : "files.description")}
      />
      {!project ? (
        <div className="space-y-3 text-sm">
          <p className="text-text-muted">{t("files.noProject", { tool: toolName })}</p>
          <button
            type="button"
            onClick={() => void choose()}
            className="rounded-lg border border-border bg-surface-raised px-3 py-1.5 text-sm hover:border-border-strong"
          >
            {t("work.chooseFolder")}
          </button>
        </div>
      ) : (
        <>
          <div className="mb-3 flex items-center gap-2 text-xs text-text-muted">
            <span className="selectable min-w-0 flex-1 truncate" title={project}>
              {project}
            </span>
            <button
              type="button"
              onClick={() => void choose()}
              className="shrink-0 rounded-md border border-border bg-surface-raised px-2 py-1 hover:border-border-strong"
            >
              {t("docs.project.choose")}
            </button>
            <button
              type="button"
              onClick={() => void refresh()}
              title={t("work.refresh")}
              className="shrink-0 rounded p-1 hover:bg-hover hover:text-text"
            >
              <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
            </button>
          </div>
          {error && (
            <div className="mb-3 flex items-start gap-2 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">
              <p className="selectable min-w-0 flex-1 whitespace-pre-wrap break-words">{error}</p>
              <button type="button" aria-label={t("work.close")} onClick={dismissError}>
                <X size={13} />
              </button>
            </div>
          )}
          <div className="flex min-h-0 flex-1 gap-4">
            <div className="relative shrink-0" style={{ width: docsPanel.width }}>
              <ResizeHandle side="right" {...docsPanel.handle} />
              <aside className="h-full space-y-3 overflow-y-auto pr-2">
                <button
                  type="button"
                  onClick={() => setCreating(!creating)}
                  className="flex w-full items-center gap-1.5 rounded-md border border-border bg-surface-raised px-2.5 py-1.5 text-xs hover:border-border-strong"
                >
                  <Plus size={13} />
                  {t("files.new")}
                </button>
                {creating && <NewDocForm tool={tool} onDone={() => setCreating(false)} />}
                {!loading && entries.length === 0 && (
                  <p className="text-xs text-text-muted">{t("files.empty", { tool: toolName })}</p>
                )}
                {KINDS.map(({ kind, label }) => {
                  const items = entries.filter((e) => e.kind === kind);
                  if (!items.length) return null;
                  return (
                    <section key={kind}>
                      <h3 className="mb-1 text-fs-11 font-semibold text-text-muted">
                        {t(label)} <span className="font-normal">{items.length}</span>
                      </h3>
                      <ul className="space-y-px">
                        {items.map((entry) => (
                          <DocRow key={entry.rel} entry={entry} active={entry.rel === rel} />
                        ))}
                      </ul>
                    </section>
                  );
                })}
              </aside>
            </div>
            <div className="flex min-w-0 flex-1 flex-col">
              {rel ? (
                <Editor project={project} />
              ) : (
                <p className="text-xs text-text-muted">{t("files.pick")}</p>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

/** Asks before unsaved edits are dropped; true when it's fine to go on. */
async function leaveEdits(message: string, title: string): Promise<boolean> {
  const { text, saved } = useProjectDocsStore.getState();
  if (text === saved) return true;
  const ok = await ask(message, { title, kind: "warning" }).catch(() => false);
  if (ok) useProjectDocsStore.setState({ text: saved });
  return ok;
}

function DocRow({ entry, active }: { entry: DocEntry; active: boolean }) {
  const t = useT();
  const open = useProjectDocsStore((s) => s.open);
  const slash = entry.rel.lastIndexOf("/");
  const name = entry.rel.slice(slash + 1);
  const dir = slash > 0 ? entry.rel.slice(0, slash) : "";
  return (
    <li>
      <button
        type="button"
        onClick={async () => {
          if (await leaveEdits(t("files.discardConfirm"), t("files.discardTitle"))) {
            void open(entry.rel);
          }
        }}
        title={entry.rel}
        className={`flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-xs hover:bg-hover ${active ? "bg-hover font-medium" : ""}`}
      >
        <FileText size={12} className="shrink-0 text-text-subtle" />
        <span className="min-w-0 flex-1 truncate">
          {entry.kind === "skill" && name === "SKILL.md" ? (dir.split("/").pop() ?? name) : name}
          {dir && entry.kind !== "skill" && (
            <span className="ml-1 font-normal text-text-subtle">{dir}</span>
          )}
        </span>
      </button>
    </li>
  );
}

function NewDocForm({ tool, onDone }: { tool: ToolId; onDone: () => void }) {
  const t = useT();
  const create = useProjectDocsStore((s) => s.create);
  const kinds = NEW_KINDS[tool];
  const [picked, setKind] = useState<NewKind>("instructions");
  const kind = kinds.includes(picked) ? picked : "instructions";
  const [name, setName] = useState("");
  const doc = newDoc(tool, kind, name);
  const input =
    "w-full min-w-0 rounded-md border border-border bg-surface px-2 py-1 text-xs outline-none placeholder:text-text-subtle focus:border-border-strong";

  return (
    <form
      onSubmit={async (e) => {
        e.preventDefault();
        if (!doc) return;
        if (!(await leaveEdits(t("files.discardConfirm"), t("files.discardTitle")))) return;
        if (await create(doc.rel, doc.text)) {
          setName("");
          onDone();
        }
      }}
      className="space-y-1.5 rounded-lg border border-border bg-surface-raised p-2"
    >
      <select
        aria-label={t("files.new.kind")}
        value={kind}
        onChange={(e) => setKind(e.target.value as NewKind)}
        className={input}
      >
        {kinds.map((k) => {
          const label = newKindLabel(tool, k);
          return (
            <option key={k} value={k}>
              {label.startsWith("docs.") ? t(label as MessageKey) : label}
            </option>
          );
        })}
      </select>
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder={t(IN_FOLDER.includes(kind) ? "files.new.folder" : "files.new.name")}
        className={input}
      />
      {doc && <p className="truncate text-fs-11 text-text-subtle">{doc.rel}</p>}
      <div className="flex justify-end gap-1.5 text-xs">
        <button
          type="button"
          onClick={onDone}
          className="rounded-md px-2 py-1 text-text-muted hover:bg-hover"
        >
          {t("work.cancel")}
        </button>
        <button
          type="submit"
          disabled={!doc}
          className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          {t("work.create")}
        </button>
      </div>
    </form>
  );
}

function Editor({ project }: { project: string }) {
  const t = useT();
  const { rel, text, saved, changedOutside, saving, edit, save, reload, remove } =
    useProjectDocsStore();
  const dirty = text !== saved;

  // Ctrl+S / Cmd+S saves.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void useProjectDocsStore.getState().save();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!rel) return null;
  const button =
    "flex shrink-0 items-center gap-1 rounded-md border border-border bg-surface-raised px-2 py-1 text-xs hover:border-border-strong disabled:opacity-50";

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex items-center gap-1.5">
        <span className="selectable min-w-0 flex-1 truncate font-mono text-xs" title={rel}>
          {rel}
          {dirty && <span className="ml-1.5 text-warning">●</span>}
        </span>
        <button
          type="button"
          onClick={() => void revealItemInDir(joinPath(project, rel)).catch(() => {})}
          title={t("files.reveal")}
          className={button}
        >
          <FolderOpen size={12} />
        </button>
        <button
          type="button"
          onClick={async () => {
            const ok = await ask(t("files.deleteConfirm", { rel }), {
              title: t("files.deleteTitle"),
              kind: "warning",
            }).catch(() => false);
            if (ok) await remove(rel);
          }}
          title={t("files.delete")}
          className={`${button} hover:text-danger`}
        >
          <Trash2 size={12} />
        </button>
        <button type="button" disabled={!dirty} onClick={() => void reload()} className={button}>
          <RotateCcw size={12} />
          {t("files.revert")}
        </button>
        <button
          type="button"
          disabled={!dirty || saving}
          onClick={() => void save()}
          className="flex shrink-0 items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
          title="Ctrl+S"
        >
          <Save size={12} />
          {t(saving ? "files.saving" : "files.save")}
        </button>
      </div>
      {changedOutside && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg bg-warning/15 px-3 py-2 text-xs">
          <span className="min-w-0 flex-1">{t("files.changedOutside")}</span>
          <button type="button" onClick={() => void reload()} className={button}>
            {t("files.useTheirs")}
          </button>
          <button type="button" onClick={() => void save(true)} className={button}>
            {t("files.keepMine")}
          </button>
        </div>
      )}
      <textarea
        value={text}
        onChange={(e) => edit(e.target.value)}
        spellCheck={false}
        aria-label={rel}
        className="selectable min-h-[60vh] w-full flex-1 resize-none rounded-lg border border-border bg-surface-raised p-3 font-mono text-fs-13 leading-relaxed outline-none focus:border-border-strong"
      />
    </div>
  );
}
