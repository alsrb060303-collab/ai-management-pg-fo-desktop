import { isTauri } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { type MessageKey, useT } from "../../i18n";
import { useLibraryStore } from "../../store/library";
import {
  type ConfigItem,
  type ConfigKind,
  type ConfigScope,
  type ConfigState,
  useProjectStore,
} from "../../store/project";
import { ModeButtons, smallButton } from "./ModeButtons";

/** `hint` says in one line what the kind does; `path` (where it lives) is the tooltip. */
const SECTIONS: { kind: ConfigKind; title: MessageKey; hint: MessageKey; path: string }[] = [
  {
    kind: "instructions",
    title: "docs.section.instructions",
    hint: "docs.section.instructionsHint",
    path: "CLAUDE.md",
  },
  {
    kind: "rule",
    title: "docs.section.rule",
    hint: "docs.section.ruleHint",
    path: ".claude/rules",
  },
  {
    kind: "skill",
    title: "docs.section.skill",
    hint: "docs.section.skillHint",
    path: ".claude/skills",
  },
  {
    kind: "agent",
    title: "docs.section.agent",
    hint: "docs.section.agentHint",
    path: ".claude/agents",
  },
  {
    kind: "command",
    title: "docs.section.command",
    hint: "docs.section.commandHint",
    path: ".claude/commands",
  },
  { kind: "mcp", title: "docs.section.mcp", hint: "docs.section.mcpHint", path: ".mcp.json" },
  {
    kind: "plugin",
    title: "docs.section.plugin",
    hint: "docs.section.pluginHint",
    path: "enabledPlugins",
  },
  {
    kind: "settings",
    title: "docs.section.settings",
    hint: "docs.section.settingsHint",
    path: "settings.json",
  },
];

/** What each scope means, shown when an item is opened. */
const SCOPE_HELP: Record<ConfigScope, MessageKey> = {
  managed: "docs.scopeHelp.managed",
  user: "docs.scopeHelp.user",
  parent: "docs.scopeHelp.parent",
  project: "docs.scopeHelp.project",
  local: "docs.scopeHelp.local",
};

const STATE_HELP: Record<ConfigState, MessageKey> = {
  active: "docs.stateHelp.active",
  conditional: "docs.stateHelp.conditional",
  limited: "docs.stateHelp.limited",
  off: "docs.stateHelp.off",
  shadowed: "docs.stateHelp.shadowed",
  notLoaded: "docs.stateHelp.notLoaded",
};

/** The first sentence of a (long, often English) description. */
function firstSentence(text: string): string {
  const t = text.trim();
  const m = /^(.+?[.!?])(\s|$)/s.exec(t);
  const line = (m ? m[1] : t).split("\n")[0];
  return line ?? t;
}

const SCOPE_LABELS: Record<ConfigScope, MessageKey> = {
  managed: "docs.scope.managed",
  user: "docs.scope.user",
  parent: "docs.scope.parent",
  project: "docs.scope.project",
  local: "docs.scope.local",
};

const STATE_STYLES: Record<ConfigState, { label: MessageKey; className: string }> = {
  active: { label: "docs.state.active", className: "text-success" },
  conditional: { label: "docs.state.conditional", className: "text-info" },
  limited: { label: "docs.state.limited", className: "text-warning" },
  off: { label: "docs.state.off", className: "text-text-muted" },
  shadowed: { label: "docs.state.shadowed", className: "text-text-muted" },
  notLoaded: { label: "docs.state.notLoaded", className: "text-text-muted" },
};

export function ConfigMapPanel({ onEdit }: { onEdit?: (rel: string) => void } = {}) {
  const t = useT();
  const { current, recent, map, loading, error, choose, select, refresh } = useProjectStore();
  const loadLibrary = useLibraryStore((s) => s.load);
  const libraryError = useLibraryStore((s) => s.error);

  // The library knows which items this app turned off in the project.
  useEffect(() => {
    void current;
    void loadLibrary();
  }, [current, loadLibrary]);

  return (
    <div className="space-y-3">
      <ProjectBar
        current={current}
        recent={recent}
        loading={loading}
        onChoose={choose}
        onSelect={select}
        onRefresh={refresh}
      />
      {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-xs text-danger">{error}</p>}
      {libraryError && (
        <p className="rounded-md bg-danger-soft px-3 py-2 text-xs text-danger">{libraryError}</p>
      )}
      {current && loading && !map && (
        <p className="text-xs text-text-muted">{t("docs.map.reading")}</p>
      )}
      {!current && <p className="text-xs text-text-muted">{t("docs.map.pickProject")}</p>}
      {map &&
        SECTIONS.map((section) => {
          const items = map.items.filter((i) => i.kind === section.kind);
          if (items.length === 0) return null;
          return (
            <section key={section.kind}>
              <h3
                className="mb-1.5 flex items-baseline gap-2 text-xs font-semibold"
                title={section.path}
              >
                {t(section.title)}
                <span className="font-normal text-text-subtle">{items.length}</span>
                <span className="min-w-0 truncate font-normal text-text-muted">
                  {t(section.hint)}
                </span>
              </h3>
              <ul className="space-y-1.5">
                {items.map((item) => (
                  <ConfigRow
                    key={`${item.scope}:${item.path}:${item.name}`}
                    item={item}
                    editRel={onEdit && current ? relInProject(current, item.path) : null}
                    onEdit={onEdit}
                  />
                ))}
              </ul>
            </section>
          );
        })}
      {map && map.items.length === 0 && (
        <p className="text-xs text-text-muted">{t("docs.map.empty")}</p>
      )}
      {map && map.notes.length > 0 && (
        <ul className="space-y-0.5 border-t border-border pt-2 text-[11px] text-text-muted">
          {map.notes.map((note) => (
            <li key={note}>· {note}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface ProjectBarProps {
  current: string | null;
  recent: string[];
  loading: boolean;
  onChoose: () => void;
  onSelect: (path: string) => void;
  onRefresh: () => void;
}

function ProjectBar({ current, recent, loading, onChoose, onSelect, onRefresh }: ProjectBarProps) {
  const t = useT();
  return (
    <div className="flex items-center gap-1.5">
      <select
        aria-label={t("docs.project.recent")}
        value={current ?? ""}
        onChange={(e) => e.target.value && onSelect(e.target.value)}
        className="min-w-0 flex-1 truncate rounded-md border border-border bg-surface-raised px-2 py-1.5 text-xs"
        title={current ?? undefined}
      >
        {!current && <option value="">{t("docs.project.none")}</option>}
        {(current && !recent.includes(current) ? [current, ...recent] : recent).map((path) => (
          <option key={path} value={path}>
            {baseName(path)}
          </option>
        ))}
      </select>
      <button type="button" onClick={onChoose} className={buttonClass}>
        {t("docs.project.choose")}
      </button>
      <button
        type="button"
        onClick={onRefresh}
        disabled={!current || loading}
        className={buttonClass}
        title={t("docs.project.refresh")}
      >
        {loading ? "…" : "↻"}
      </button>
    </div>
  );
}

function ConfigRow({
  item,
  editRel,
  onEdit,
}: {
  item: ConfigItem;
  editRel?: string | null;
  onEdit?: (rel: string) => void;
}) {
  const t = useT();
  const state = STATE_STYLES[item.state];
  const dimmed = item.state === "shadowed" || item.state === "off" || item.state === "notLoaded";
  const { busy, setEnabled, importItem, overview } = useLibraryStore();

  const [failed, setFailed] = useState(false);
  const [open, setOpen] = useState(false);
  const reveal = () => {
    if (!isTauri() || !item.path) return;
    revealItemInDir(item.path).then(
      () => setFailed(false),
      () => setFailed(true),
    );
  };

  const kind = item.kind === "skill" || item.kind === "agent" ? item.kind : null;
  // Only switches this app can flip: not managed policy, synced or broken items.
  const switchable =
    kind !== null &&
    item.scope !== "managed" &&
    !item.name.startsWith("anthropic-skills:") &&
    item.state !== "notLoaded";
  // Only offs this app made can be undone; rules the user wrote are left alone.
  const toggled = overview?.toggledOff.find((o) => o.kind === kind && o.name === item.name);
  const canTurnOff =
    switchable && !toggled && (item.state === "active" || item.state === "limited");
  const canImport = kind !== null && (item.scope === "user" || item.scope === "project");

  return (
    <li className="rounded-md border border-border bg-surface-raised hover:border-border-strong">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className="block w-full px-2.5 py-1.5 text-left"
      >
        <div className="flex items-center gap-1.5 text-xs">
          <span
            className={`min-w-0 flex-1 truncate font-medium ${dimmed ? "text-text-muted line-through decoration-text-muted/60" : ""}`}
          >
            {item.name}
          </span>
          <span className="shrink-0 rounded bg-sunken px-1 text-[10px] text-text-muted">
            {t(SCOPE_LABELS[item.scope])}
          </span>
          <span className={`shrink-0 text-[10px] ${state.className}`}>{t(state.label)}</span>
        </div>
        {!open && (item.description || item.detail) && (
          <p className="mt-0.5 truncate text-[11px] text-text-muted">
            {item.description ? firstSentence(item.description) : item.detail}
          </p>
        )}
      </button>
      {open && (
        <div className="space-y-1.5 px-2.5 pb-2 text-[11px] leading-relaxed">
          {item.description && <p className="text-text">{item.description}</p>}
          {item.detail && <p className="text-text-muted">{item.detail}</p>}
          <p className="text-text-muted">
            <span className="font-medium text-text">{t(state.label)}</span> ·{" "}
            {t(STATE_HELP[item.state])}
          </p>
          <p className="text-text-muted">
            <span className="font-medium text-text">{t(SCOPE_LABELS[item.scope])}</span> ·{" "}
            {t(SCOPE_HELP[item.scope])}
          </p>
          {item.path && (
            <div className="flex flex-wrap items-center gap-1">
              <span className="selectable min-w-0 flex-1 truncate font-mono text-[10px] text-text-subtle">
                {item.path}
              </span>
              <button type="button" className={smallButton} onClick={reveal}>
                {t("docs.item.reveal")}
              </button>
            </div>
          )}
          {failed && <p className="text-[10px] text-danger">{t("docs.item.openFailed")}</p>}
        </div>
      )}
      {((kind && (switchable || canImport)) || (editRel && onEdit)) && (
        <div className="flex flex-wrap gap-1 px-2.5 pb-1.5">
          {editRel && onEdit && (
            <button type="button" className={smallButton} onClick={() => onEdit(editRel)}>
              {t("files.edit")}
            </button>
          )}
          {toggled && kind && (
            <button
              type="button"
              className={smallButton}
              disabled={busy}
              onClick={() => setEnabled(kind, item.name, true, toggled.mode)}
            >
              {t(toggled.mode === "personal" ? "docs.item.turnOnPersonal" : "docs.item.turnOnTeam")}
            </button>
          )}
          {canTurnOff && kind && (
            <ModeButtons
              label={t("docs.item.turnOff")}
              disabled={busy}
              // A personal skill isn't in teammates' checkouts, so a team rule would mean nothing.
              personalOnly={item.scope === "user"}
              onPick={(mode) => setEnabled(kind, item.name, false, mode)}
            />
          )}
          {canImport && kind && (
            <button
              type="button"
              className={smallButton}
              disabled={busy}
              onClick={() => importItem(item.path, kind)}
            >
              {t("docs.item.import")}
            </button>
          )}
        </div>
      )}
    </li>
  );
}

const buttonClass =
  "shrink-0 rounded-md border border-border bg-surface-raised px-2 py-1.5 text-xs hover:border-border-strong disabled:opacity-50";

/** `path` relative to `project` (`/`-separated), or `null` outside it. */
function relInProject(project: string, path: string): string | null {
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");
  const root = `${norm(project)}/`;
  const file = norm(path);
  const inside = /^[a-z]:/i.test(root)
    ? file.toLowerCase().startsWith(root.toLowerCase())
    : file.startsWith(root);
  return inside && /\.(md|json)$/.test(file) ? file.slice(root.length) : null;
}

function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
