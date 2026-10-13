import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ClipboardPaste,
  ExternalLink,
  Eye,
  EyeOff,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { Button, IconButton, PageHeader, Switch } from "../../components/ui";
import { type MessageKey, useT } from "../../i18n";
import {
  type Draft,
  emptyConfig,
  type Health,
  looksSecret,
  type McpConfig,
  type McpScope,
  type McpServer,
  type Parsed,
  type Recommended,
  type Transport,
  useMcpStore,
  variableFor,
} from "../../store/mcp";
import { useProjectStore } from "../../store/project";
import type { ToolId } from "../../tools";

type EditableScope = Exclude<McpScope, "other">;

/** Per tool: scopes listed, scopes a server can be saved in, transports, and
 * where a new server goes. Codex has no "local" scope (one config.toml per
 * project, shared) and no plugin/connector servers. */
const TOOL_MCP: Record<
  ToolId,
  {
    scopes: McpScope[];
    editable: EditableScope[];
    transports: Transport[];
    defaultScope: EditableScope;
  }
> = {
  claude: {
    scopes: ["project", "local", "user", "other"],
    editable: ["project", "local", "user"],
    transports: ["stdio", "http", "sse", "ws"],
    defaultScope: "local",
  },
  codex: {
    scopes: ["project", "user"],
    editable: ["project", "user"],
    transports: ["stdio", "http"],
    defaultScope: "user",
  },
};

/** The scope's message key: Codex scopes name its own files. */
const scopeKey = (tool: ToolId, kind: "scope" | "scopeShort" | "scopeHint", scope: McpScope) =>
  (tool === "codex" && kind !== "scopeShort"
    ? `mcp.codex.${kind}.${scope}`
    : `mcp.${kind}.${scope}`) as MessageKey;

const HEALTH_DOT: Record<Health, string> = {
  connected: "bg-success",
  failed: "bg-danger",
  pending: "bg-warning",
  needsAuth: "bg-warning",
  off: "bg-border-strong",
  unknown: "bg-border-strong",
  noCheck: "bg-border-strong",
};

/** MCP servers of the current project, every scope, with their status. */
export function McpPage() {
  const t = useT();
  const project = useProjectStore((s) => s.current);
  const choose = useProjectStore((s) => s.choose);
  const tool = useProjectStore((s) => s.tool);
  // Per-tool data waits until a newly selected project's tool is known.
  const toolReady = useProjectStore((s) => s.toolReady || !s.current);
  const { servers, cliError, projectError, userError, trust, loading, error, load, dismissError } =
    useMcpStore();
  const [draft, setDraft] = useState<Draft | null>(null);
  const [importing, setImporting] = useState(false);
  const info = TOOL_MCP[tool];

  useEffect(() => {
    if (toolReady) void load(project, tool);
  }, [project, tool, toolReady, load]);

  const newDraft = (): Draft => ({
    original: null,
    scope: info.defaultScope,
    name: "",
    config: emptyConfig(),
  });

  return (
    <div className="mx-auto max-w-4xl px-8 py-10">
      <PageHeader
        title={t("mcp.title")}
        description={t(tool === "codex" ? "mcp.codex.description" : "mcp.description")}
        actions={
          project && (
            <>
              <Button size="sm" onClick={() => setImporting(true)}>
                <ClipboardPaste size={13} />
                {t("mcp.import")}
              </Button>
              <Button size="sm" variant="primary" onClick={() => setDraft(newDraft())}>
                <Plus size={13} />
                {t("mcp.add")}
              </Button>
            </>
          )
        }
      />
      {!project ? (
        <div className="space-y-3 text-sm">
          <p className="text-text-muted">{t("mcp.noProject")}</p>
          <Button onClick={() => void choose()}>{t("work.chooseFolder")}</Button>
        </div>
      ) : (
        <>
          <div className="mb-4 flex items-center gap-2 text-xs text-text-muted">
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
              onClick={() => void load(project, tool)}
              title={t("mcp.refresh")}
              className="flex shrink-0 items-center gap-1 rounded p-1 hover:bg-hover hover:text-text"
            >
              <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
              {loading && t(tool === "codex" ? "mcp.codex.reading" : "mcp.checking")}
            </button>
          </div>
          {[error, projectError, userError].map(
            (msg) =>
              msg && (
                <div
                  key={msg}
                  className="mb-3 flex items-start gap-2 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger"
                >
                  <p className="selectable min-w-0 flex-1 whitespace-pre-wrap break-words">{msg}</p>
                  {msg === error && (
                    <button type="button" aria-label={t("work.close")} onClick={dismissError}>
                      <X size={13} />
                    </button>
                  )}
                </div>
              ),
          )}
          {cliError && (
            <p className="mb-3 rounded-lg bg-warning/10 px-3 py-2 text-xs text-warning">
              {t("mcp.cliError")}
              <span className="selectable mt-1 block text-text-subtle">{cliError}</span>
            </p>
          )}

          <div className="space-y-5">
            {info.scopes.map((scope) => {
              const list = servers.filter((s) => s.scope === scope);
              if (scope === "other" && !list.length) return null;
              const trustHint =
                tool === "codex" && scope === "project" && trust && trust !== "trusted"
                  ? t(trust === "untrusted" ? "mcp.codex.untrusted" : "mcp.codex.trustUnknown")
                  : null;
              return (
                <section key={scope}>
                  <h3 className="mb-1.5 text-xs font-semibold text-text-muted">
                    {t(scopeKey(tool, "scope", scope))}{" "}
                    <span className="font-normal">{list.length}</span>
                  </h3>
                  {trustHint && (
                    <p className="mb-1.5 rounded-lg bg-warning/10 px-3 py-2 text-xs text-warning">
                      {trustHint}
                    </p>
                  )}
                  {list.length === 0 ? (
                    <p className="rounded-lg border border-dashed border-border px-3 py-2.5 text-xs text-text-subtle">
                      {loading ? t("mcp.checking") : t("mcp.empty")}
                    </p>
                  ) : (
                    <ul className="divide-y divide-border rounded-xl border border-border bg-surface-raised">
                      {list.map((s) => (
                        <ServerRow
                          key={`${s.scope}:${s.name}`}
                          server={s}
                          tool={tool}
                          project={project}
                          onEdit={() =>
                            s.config &&
                            setDraft({
                              original: { scope: s.scope, name: s.name, config: s.config },
                              scope: s.scope,
                              name: s.name,
                              config: s.config,
                              argsApprox: s.argsApprox,
                            })
                          }
                        />
                      ))}
                    </ul>
                  )}
                </section>
              );
            })}
          </div>

          <RecommendedList
            tool={tool}
            taken={new Set(servers.map((s) => s.name))}
            onPick={(r) =>
              setDraft({
                original: null,
                scope: info.defaultScope,
                name: r.name,
                config: fillProject(r.config, project),
              })
            }
          />
        </>
      )}

      {draft && project && (
        <EditDialog project={project} tool={tool} initial={draft} onClose={() => setDraft(null)} />
      )}
      {importing && (
        <ImportDialog
          onClose={() => setImporting(false)}
          onPick={(p) => {
            setImporting(false);
            setDraft({
              original: null,
              scope:
                p.scope && (info.editable as McpScope[]).includes(p.scope)
                  ? p.scope
                  : info.defaultScope,
              name: p.name,
              config: p.config,
            });
          }}
        />
      )}
    </div>
  );
}

/** Recommended configs use `{project}` for the project folder. */
function fillProject(config: McpConfig, project: string): McpConfig {
  return { ...config, args: config.args.map((a) => a.split("{project}").join(project)) };
}

function ServerRow({
  server: s,
  tool,
  project,
  onEdit,
}: {
  server: McpServer;
  tool: ToolId;
  project: string;
  onEdit: () => void;
}) {
  const t = useT();
  const { remove, setEnabled } = useMcpStore.getState();
  const editable = s.scope !== "other" && s.config !== null;
  const hint =
    s.health === "pending"
      ? t("mcp.pendingHint")
      : s.health === "needsAuth"
        ? t("mcp.authHint")
        : s.health === "failed"
          ? s.status.replace(/^[×✗]\s*/, "")
          : "";
  return (
    <li className="flex items-center gap-3 px-3.5 py-2.5">
      <span
        className={`size-2 shrink-0 rounded-full ${HEALTH_DOT[s.health]}`}
        title={s.status || t(`mcp.health.${s.health}` as MessageKey)}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{s.name}</span>
          <span className="rounded bg-sunken px-1.5 text-fs-10 text-text-subtle">
            {s.config?.transport ?? "—"}
          </span>
          <span className="text-fs-11 text-text-muted">
            {t(`mcp.health.${s.health}` as MessageKey)}
          </span>
        </div>
        <div className="selectable truncate font-mono text-fs-11 text-text-subtle" title={s.target}>
          {s.target}
        </div>
        {hint && <p className="mt-0.5 text-fs-11 text-text-muted">{hint}</p>}
      </div>
      {(tool === "codex" ? s.enabled !== null : s.scope === "project") && (
        <span title={t(tool === "codex" ? "mcp.codex.toggle" : "mcp.toggle")}>
          <Switch
            checked={s.enabled === true}
            label={t(tool === "codex" ? "mcp.codex.toggle" : "mcp.toggle")}
            onChange={(on) => void setEnabled(project, s, on)}
          />
        </span>
      )}
      {editable && (
        <>
          <IconButton label={t("mcp.edit")} onClick={onEdit}>
            <Pencil size={14} />
          </IconButton>
          <IconButton
            label={t("mcp.delete")}
            onClick={async () => {
              const ok = await ask(t("mcp.deleteConfirm", { name: s.name }), {
                title: t("mcp.deleteTitle"),
                kind: "warning",
              }).catch(() => false);
              if (ok) await remove(project, s);
            }}
          >
            <Trash2 size={14} />
          </IconButton>
        </>
      )}
    </li>
  );
}

function RecommendedList({
  tool,
  taken,
  onPick,
}: {
  tool: ToolId;
  taken: Set<string>;
  onPick: (r: Recommended) => void;
}) {
  const t = useT();
  const recommended = useMcpStore((s) => s.recommended);
  if (!recommended.length) return null;
  return (
    <section className="mt-8">
      <h3 className="text-xs font-semibold text-text-muted">{t("mcp.rec.title")}</h3>
      <p className="mb-2 text-fs-11 text-text-subtle">
        {t("mcp.rec.hint")}
        {tool === "codex" && ` ${t("mcp.codex.recHint")}`}
      </p>
      <div className="grid grid-cols-2 gap-2">
        {recommended.map((r) => {
          const added = taken.has(r.name);
          return (
            <div
              key={r.id}
              className="flex items-start gap-2 rounded-xl border border-border bg-surface-raised px-3 py-2.5"
            >
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="text-sm font-medium">{r.name}</span>
                  {r.repo ? (
                    <button
                      type="button"
                      onClick={() => void openUrl(`https://github.com/${r.repo}`)}
                      title={r.repo}
                      className="flex items-center gap-0.5 text-fs-10 text-text-subtle hover:text-text"
                    >
                      {r.license}
                      <ExternalLink size={9} />
                    </button>
                  ) : (
                    <span className="text-fs-10 text-text-subtle">{t("mcp.rec.hosted")}</span>
                  )}
                  {r.needs && (
                    <span className="rounded bg-sunken px-1 text-fs-10 text-text-subtle">
                      {t(`mcp.rec.needs.${r.needs}` as MessageKey)}
                    </span>
                  )}
                </div>
                <p className="mt-0.5 text-fs-11 leading-snug text-text-muted">
                  {t(`mcp.rec.${r.id}` as MessageKey)}
                </p>
              </div>
              <Button size="sm" disabled={added} onClick={() => onPick(r)}>
                {added ? t("mcp.rec.added") : t("mcp.rec.add")}
              </Button>
            </div>
          );
        })}
      </div>
    </section>
  );
}

function Dialog({
  title,
  onClose,
  children,
  footer,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer: ReactNode;
}) {
  const t = useT();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-6"
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="flex max-h-full w-full max-w-xl flex-col rounded-2xl border border-border bg-surface-raised shadow-card"
      >
        <header className="flex items-center gap-2 border-b border-border px-5 py-3.5">
          <h2 className="font-serif text-lg font-medium">{title}</h2>
          <IconButton label={t("work.close")} onClick={onClose} className="ml-auto">
            <X size={17} />
          </IconButton>
        </header>
        <div className="min-h-0 flex-1 space-y-3.5 overflow-y-auto px-5 py-4">{children}</div>
        <footer className="flex items-center justify-end gap-2 border-t border-border px-5 py-3">
          {footer}
        </footer>
      </div>
    </div>
  );
}

const box =
  "min-w-0 rounded-lg border border-border bg-surface px-2.5 py-1.5 text-sm outline-none focus:border-accent";
const input = `w-full ${box}`;

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div>
      <span className="mb-1 block text-xs font-medium text-text-muted">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-fs-11 text-text-subtle">{hint}</span>}
    </div>
  );
}

function EditDialog({
  project,
  tool,
  initial,
  onClose,
}: {
  project: string;
  tool: ToolId;
  initial: Draft;
  onClose: () => void;
}) {
  const t = useT();
  const save = useMcpStore((s) => s.save);
  const info = TOOL_MCP[tool];
  const [scope, setScope] = useState<McpScope>(
    (info.editable as McpScope[]).includes(initial.scope) ? initial.scope : info.defaultScope,
  );
  const [name, setName] = useState(initial.name);
  // A pasted SSE/WebSocket server becomes HTTP where the tool has no such transport.
  const [config, setConfig] = useState<McpConfig>(
    info.transports.includes(initial.config.transport)
      ? initial.config
      : { ...initial.config, transport: "http" },
  );
  const [argsText, setArgsText] = useState(initial.config.args.join("\n"));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stdio = config.transport === "stdio";
  const patch = (p: Partial<McpConfig>) => setConfig({ ...config, ...p });

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const args = argsText
        .split("\n")
        .map((a) => a.trim())
        .filter(Boolean);
      await save(project, { ...initial, scope, name: name.trim(), config: { ...config, args } });
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={initial.original ? t("mcp.form.titleEdit") : t("mcp.form.titleNew")}
      onClose={onClose}
      footer={
        <>
          {error && (
            <p className="selectable mr-auto min-w-0 flex-1 text-xs whitespace-pre-wrap text-danger">
              {error}
            </p>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t("mcp.form.cancel")}
          </Button>
          <Button variant="primary" disabled={busy || !name.trim()} onClick={() => void submit()}>
            {busy ? t("mcp.form.saving") : t("mcp.form.save")}
          </Button>
        </>
      }
    >
      <Field label={t("mcp.form.name")} hint={t("mcp.form.nameHint")}>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          spellCheck={false}
          className={`${input} font-mono`}
        />
      </Field>
      <Field label={t("mcp.form.scope")} hint={t(scopeKey(tool, "scopeHint", scope))}>
        <div className="flex gap-1 rounded-lg bg-sunken p-0.5 text-xs">
          {info.editable.map((s) => (
            <button
              key={s}
              type="button"
              aria-pressed={scope === s}
              onClick={() => setScope(s)}
              className={`flex-1 rounded-md px-2 py-1 font-medium ${
                scope === s ? "bg-surface-raised text-text shadow-card" : "text-text-muted"
              }`}
            >
              {t(scopeKey(tool, "scopeShort", s))}
            </button>
          ))}
        </div>
      </Field>
      <Field label={t("mcp.form.transport")}>
        <select
          value={config.transport}
          onChange={(e) => patch({ transport: e.target.value as Transport })}
          className={input}
        >
          {info.transports.map((tr) => (
            <option key={tr} value={tr}>
              {t(`mcp.form.transport.${tr}` as MessageKey)}
            </option>
          ))}
        </select>
      </Field>
      {stdio ? (
        <>
          <Field label={t("mcp.form.command")}>
            <input
              value={config.command}
              onChange={(e) => patch({ command: e.target.value })}
              placeholder="npx"
              spellCheck={false}
              className={`${input} font-mono`}
            />
          </Field>
          <Field
            label={t("mcp.form.args")}
            hint={initial.argsApprox ? t("mcp.form.argsApprox") : undefined}
          >
            <textarea
              value={argsText}
              onChange={(e) => setArgsText(e.target.value)}
              rows={3}
              placeholder={"-y\n@scope/package"}
              spellCheck={false}
              className={`${input} resize-y font-mono text-xs`}
            />
          </Field>
          <Pairs
            label={t("mcp.form.env")}
            kind="env"
            server={name}
            shared={scope === "project"}
            value={config.env}
            onChange={(env) => patch({ env })}
          />
        </>
      ) : (
        <>
          <Field label={t("mcp.form.url")}>
            <input
              value={config.url}
              onChange={(e) => patch({ url: e.target.value })}
              placeholder="https://example.com/mcp"
              spellCheck={false}
              className={`${input} font-mono`}
            />
          </Field>
          <Pairs
            label={t("mcp.form.headers")}
            kind="header"
            server={name}
            shared={scope === "project"}
            value={config.headers}
            onChange={(headers) => patch({ headers })}
          />
        </>
      )}
    </Dialog>
  );
}

/** Key/value rows (env or headers); values are hidden until shown. */
function Pairs({
  label,
  kind,
  server,
  shared,
  value,
  onChange,
}: {
  label: string;
  kind: "env" | "header";
  server: string;
  shared: boolean;
  value: Record<string, string>;
  onChange: (next: Record<string, string>) => void;
}) {
  const t = useT();
  const [rows, setRows] = useState<[string, string][]>(() => Object.entries(value));
  const [shown, setShown] = useState<Set<number>>(new Set());

  const commit = (next: [string, string][]) => {
    setRows(next);
    onChange(Object.fromEntries(next.filter(([k]) => k.trim()).map(([k, v]) => [k.trim(), v])));
  };

  return (
    <div>
      <div className="mb-1 flex items-center">
        <span className="text-xs font-medium text-text-muted">{label}</span>
        <button
          type="button"
          onClick={() => commit([...rows, ["", ""]])}
          className="ml-auto flex items-center gap-0.5 text-fs-11 text-text-muted hover:text-text"
        >
          <Plus size={11} />
          {t("mcp.form.addRow")}
        </button>
      </div>
      <div className="space-y-1.5">
        {rows.map(([k, v], i) => {
          const secret = looksSecret(k, v);
          const replacement = variableFor(kind, server || "MCP", k, v);
          return (
            // biome-ignore lint/suspicious/noArrayIndexKey: rows are edited in place and have no id
            <div key={i}>
              <div className="flex items-center gap-1.5">
                <input
                  value={k}
                  onChange={(e) =>
                    commit(rows.map((r, j) => (j === i ? [e.target.value, r[1]] : r)))
                  }
                  placeholder={t("mcp.form.key")}
                  spellCheck={false}
                  className={`${box} w-2/5 shrink-0 font-mono text-xs`}
                />
                <input
                  type={shown.has(i) ? "text" : "password"}
                  value={v}
                  onChange={(e) =>
                    commit(rows.map((r, j) => (j === i ? [r[0], e.target.value] : r)))
                  }
                  placeholder={t("mcp.form.value")}
                  spellCheck={false}
                  autoComplete="off"
                  className={`${box} flex-1 font-mono text-xs`}
                />
                <IconButton
                  label={shown.has(i) ? t("mcp.form.hide") : t("mcp.form.show")}
                  onClick={() => {
                    const next = new Set(shown);
                    if (next.has(i)) next.delete(i);
                    else next.add(i);
                    setShown(next);
                  }}
                >
                  {shown.has(i) ? <EyeOff size={13} /> : <Eye size={13} />}
                </IconButton>
                <IconButton
                  label={t("mcp.form.removeRow")}
                  onClick={() => commit(rows.filter((_, j) => j !== i))}
                >
                  <X size={13} />
                </IconButton>
              </div>
              {secret && (
                <div className="mt-1 rounded-md bg-warning/10 px-2 py-1.5 text-fs-11 text-warning">
                  {shared ? t("mcp.form.secretProject") : t("mcp.form.secret")}{" "}
                  <button
                    type="button"
                    onClick={() => commit(rows.map((r, j) => (j === i ? [r[0], replacement] : r)))}
                    className="font-medium underline"
                  >
                    {t("mcp.form.useVariable", { value: replacement })}
                  </button>
                  <span className="block text-text-subtle">{t("mcp.form.variableHint")}</span>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ImportDialog({ onClose, onPick }: { onClose: () => void; onPick: (p: Parsed) => void }) {
  const t = useT();
  const [text, setText] = useState("");
  const [found, setFound] = useState<Parsed[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const read = async () => {
    setError(null);
    try {
      setFound(await invoke<Parsed[]>("mcp_parse", { text }));
    } catch (e) {
      setFound(null);
      setError(String(e));
    }
  };

  return (
    <Dialog
      title={t("mcp.import.title")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("mcp.form.cancel")}
          </Button>
          <Button variant="primary" disabled={!text.trim()} onClick={() => void read()}>
            {t("mcp.import.read")}
          </Button>
        </>
      }
    >
      <p className="text-xs text-text-muted">{t("mcp.import.hint")}</p>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={7}
        spellCheck={false}
        placeholder={
          'claude mcp add --transport http github https://api.githubcopilot.com/mcp/\n\n{ "mcpServers": { … } }'
        }
        className={`${input} resize-y font-mono text-xs`}
      />
      {error && <p className="selectable text-xs text-danger">{error}</p>}
      {found &&
        (found.length === 0 ? (
          <p className="text-xs text-text-muted">{t("mcp.import.none")}</p>
        ) : (
          <ul className="divide-y divide-border rounded-lg border border-border">
            {found.map((p, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: parsed entries may share or lack names
              <li key={i} className="flex items-center gap-2 px-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium">{p.name || t("mcp.import.noName")}</div>
                  <div className="truncate font-mono text-fs-11 text-text-subtle">
                    {p.config.transport === "stdio"
                      ? [p.config.command, ...p.config.args].join(" ")
                      : p.config.url}
                  </div>
                </div>
                <Button size="sm" onClick={() => onPick(p)}>
                  {t("mcp.import.use")}
                </Button>
              </li>
            ))}
          </ul>
        ))}
    </Dialog>
  );
}
