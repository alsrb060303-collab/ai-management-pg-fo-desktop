import { ask } from "@tauri-apps/plugin-dialog";
import {
  Bot,
  Check,
  ChevronDown,
  Clock,
  FileText,
  LoaderCircle,
  Mail,
  Pencil,
  Play,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Button, IconButton, PageHeader, Switch } from "../../components/ui";
import { AutomationForm } from "../../features/automation/AutomationForm";
import { describeSchedule, when } from "../../features/automation/schedule";
import { useT } from "../../i18n";
import {
  type Automation,
  blankAutomation,
  type Item,
  type Run,
  useAutomationStore,
} from "../../store/automation";
import { useComposerStore } from "../../store/composer";
import { useWorkspaceStore } from "../../store/workspace";

function StatusIcon({ run }: { run: Run }) {
  if (run.status === "running")
    return <LoaderCircle size={13} className="shrink-0 animate-spin text-accent" />;
  if (run.status === "success") return <Check size={13} className="shrink-0 text-success" />;
  return <X size={13} className="shrink-0 text-danger" />;
}

function RunRow({ run, showName }: { run: Run; showName?: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const text = run.error ? `${run.error}\n\n${run.result}`.trim() : run.result;
  return (
    <li className="text-xs">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        disabled={!text}
        className="flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left hover:bg-hover disabled:hover:bg-transparent"
      >
        <StatusIcon run={run} />
        <span className="shrink-0 tabular-nums text-text-muted">{when(run.startedAt)}</span>
        {showName && <span className="truncate font-medium">{run.name}</span>}
        <span className="shrink-0 text-text-subtle">{t(`auto.trigger.${run.trigger}`)}</span>
        <span
          className={`min-w-0 flex-1 truncate ${run.error ? "text-danger" : "text-text-subtle"}`}
        >
          {run.status === "running" ? t("auto.status.running") : (run.error ?? run.result)}
        </span>
        {run.costUsd != null && (
          <span className="shrink-0 tabular-nums text-text-subtle">
            {t("auto.cost", { cost: run.costUsd.toFixed(2) })}
          </span>
        )}
        {text && (
          <ChevronDown
            size={13}
            className={`shrink-0 text-text-subtle transition-transform ${open ? "rotate-180" : ""}`}
          />
        )}
      </button>
      {open && text && (
        <pre className="selectable mx-1.5 mt-1 mb-2 max-h-72 overflow-auto rounded-lg bg-sunken p-2.5 font-sans text-xs whitespace-pre-wrap">
          {text}
        </pre>
      )}
    </li>
  );
}

function AutomationCard({ item, runs, onEdit }: { item: Item; runs: Run[]; onEdit: () => void }) {
  const t = useT();
  const { setEnabled, run, remove } = useAutomationStore();
  const openDoc = useComposerStore((s) => s.openDoc);
  const go = useWorkspaceStore((s) => s.go);
  const [error, setError] = useState<string | null>(null);
  const agents = Object.keys(item.agents);
  const last = runs[0];

  const act = async (f: () => Promise<void>) => {
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(String(e));
    }
  };

  const del = async () => {
    const ok = await ask(t("auto.deleteConfirm", { name: item.name }), { kind: "warning" });
    if (ok) await act(() => remove(item.id));
  };

  return (
    <article
      className={`rounded-xl border border-border bg-surface-raised shadow-card ${item.enabled ? "" : "opacity-75"}`}
    >
      <div className="flex items-start gap-3 px-4 pt-3.5">
        <span
          className={`mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg ${
            item.enabled ? "bg-accent-soft text-accent" : "bg-sunken text-text-subtle"
          }`}
        >
          <Clock size={16} />
        </span>
        <div className="min-w-0 flex-1">
          <h3 className="truncate text-sm font-semibold">{item.name}</h3>
          <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-text-muted">
            <span className="font-medium text-text">{describeSchedule(item.schedule)}</span>
            <span>
              {item.enabled && item.nextRunAt
                ? t("auto.next", { time: when(item.nextRunAt) })
                : t("auto.off")}
            </span>
            {item.model && <span className="rounded bg-sunken px-1">{item.model}</span>}
          </div>
        </div>
        <Switch
          checked={item.enabled}
          label={t("auto.enabled")}
          onChange={(on) => void act(() => setEnabled(item.id, on))}
        />
      </div>

      <p className="mx-4 mt-2 line-clamp-2 text-xs text-text-muted">{item.prompt}</p>

      <div className="mx-4 mt-2 flex flex-wrap items-center gap-1.5 text-fs-11">
        {agents.map((name) => (
          <span
            key={name}
            className="flex items-center gap-1 rounded-full px-2 py-0.5"
            style={{
              color: "var(--c-kind-agent)",
              background: "color-mix(in srgb, var(--c-kind-agent) 12%, transparent)",
            }}
          >
            <Bot size={11} />
            {name}
            {item.agents[name].model && (
              <span className="opacity-75">· {item.agents[name].model}</span>
            )}
          </span>
        ))}
        {item.allowedTools.map((tool) => (
          <span key={tool} className="rounded-full bg-sunken px-2 py-0.5 text-text-muted">
            {tool}
          </span>
        ))}
        {item.email && (
          <span className="flex items-center gap-1 rounded-full bg-sunken px-2 py-0.5 text-text-muted">
            <Mail size={11} />
            {item.email}
          </span>
        )}
        {item.docId && (
          <button
            type="button"
            onClick={() => {
              void openDoc(item.docId as string);
              go("composer");
            }}
            className="flex items-center gap-1 rounded-full px-2 py-0.5 text-accent hover:bg-accent-soft"
          >
            <FileText size={11} />
            {t("auto.openDoc")}
          </button>
        )}
      </div>

      <div className="mt-3 flex items-center gap-2 border-t border-border px-4 py-2">
        <span className="min-w-0 flex-1 truncate text-xs text-text-subtle">
          {t("auto.lastRun")}:{" "}
          {last ? (
            <span className={last.status === "failed" ? "text-danger" : ""}>
              {when(last.startedAt)} · {t(`auto.status.${last.status}`)}
            </span>
          ) : (
            t("auto.neverRan")
          )}
        </span>
        <IconButton label={t("auto.edit")} className="size-7" onClick={onEdit}>
          <Pencil size={14} />
        </IconButton>
        <IconButton label={t("auto.delete")} className="size-7" onClick={() => void del()}>
          <Trash2 size={14} />
        </IconButton>
        <Button
          size="sm"
          variant="primary"
          disabled={item.running}
          onClick={() => void act(() => run(item.id))}
        >
          {item.running ? <LoaderCircle size={13} className="animate-spin" /> : <Play size={13} />}
          {item.running ? t("auto.running") : t("auto.runNow")}
        </Button>
      </div>
      {error && <p className="selectable px-4 pb-2 text-xs text-danger">{error}</p>}
    </article>
  );
}

export function AutomationsPage() {
  const t = useT();
  const { loaded, claudeFound, items, runs, load, save } = useAutomationStore();
  const [editing, setEditing] = useState<Automation | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader
        title={t("auto.title")}
        description={t("auto.description")}
        actions={
          <Button variant="primary" onClick={() => setEditing(blankAutomation())}>
            <Plus size={15} />
            {t("auto.new")}
          </Button>
        }
      />
      {!claudeFound && <p className="mb-4 text-xs text-warning">{t("auto.noClaude")}</p>}
      {loaded && items.length === 0 && (
        <p className="rounded-xl border border-dashed border-border px-4 py-8 text-center text-sm text-text-muted">
          {t("auto.empty")}
        </p>
      )}
      <div className="space-y-3">
        {items.map((item) => (
          <AutomationCard
            key={item.id}
            item={item}
            runs={runs.filter((r) => r.automationId === item.id)}
            onEdit={() => setEditing(item)}
          />
        ))}
      </div>

      {runs.length > 0 && (
        <section className="mt-8">
          <h2 className="mb-2 px-1 text-xs font-semibold text-text-muted">{t("auto.runs")}</h2>
          <ul className="rounded-xl border border-border bg-surface-raised p-1.5">
            {runs.slice(0, 30).map((r) => (
              <RunRow key={r.id} run={r} showName />
            ))}
          </ul>
        </section>
      )}

      {editing && (
        <AutomationForm
          initial={editing}
          onSave={(a) => save(a)}
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  );
}
