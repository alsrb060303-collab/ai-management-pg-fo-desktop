import { isTauri } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Bot, FolderOpen, Mail, X } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Button, IconButton, Segmented } from "../../components/ui";
import { type MessageKey, useT } from "../../i18n";
import { type Automation, DEFAULT_MAIL_TOOL } from "../../store/automation";
import { MODELS } from "../../store/composer";
import { dayLabel } from "./schedule";

/** Tool groups offered as checkboxes; anything else goes in the free field. */
const TOOL_GROUPS: { key: string; label: MessageKey; tools: string[] }[] = [
  { key: "web", label: "auto.form.tool.web", tools: ["WebSearch", "WebFetch"] },
  { key: "read", label: "auto.form.tool.read", tools: ["Read", "Glob", "Grep"] },
  { key: "edit", label: "auto.form.tool.edit", tools: ["Edit", "Write"] },
  { key: "bash", label: "auto.form.tool.bash", tools: ["Bash"] },
];
const GROUPED = new Set(TOOL_GROUPS.flatMap((g) => g.tools));

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

/**
 * Creates or edits an automation. From the composer (`fromBlocks`), the
 * instructions and subagents come from the connected blocks and are only shown.
 */
export function AutomationForm({
  initial,
  fromBlocks,
  onSave,
  onClose,
}: {
  initial: Automation;
  fromBlocks?: { instructions: number; agents: string[] };
  onSave: (automation: Automation) => Promise<unknown>;
  onClose: () => void;
}) {
  const t = useT();
  const [a, setA] = useState<Automation>(initial);
  const [extra, setExtra] = useState(
    initial.allowedTools.filter((tool) => !GROUPED.has(tool)).join(", "),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const patch = (p: Partial<Automation>) => setA((cur) => ({ ...cur, ...p }));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const groupOn = (tools: string[]) => tools.every((x) => a.allowedTools.includes(x));
  const toggleGroup = (tools: string[]) =>
    patch({
      allowedTools: groupOn(tools)
        ? a.allowedTools.filter((x) => !tools.includes(x))
        : [...a.allowedTools.filter((x) => !tools.includes(x)), ...tools],
    });

  const pickFolder = async () => {
    if (!isTauri()) return;
    const dir = await openDialog({ directory: true, defaultPath: a.project || undefined });
    if (typeof dir === "string") patch({ project: dir });
  };

  const submit = async () => {
    setBusy(true);
    setError(null);
    const tools = [
      ...a.allowedTools.filter((x) => GROUPED.has(x)),
      ...extra
        .split(/[,\s]+/)
        .map((x) => x.trim())
        .filter(Boolean),
    ];
    try {
      await onSave({ ...a, allowedTools: tools, mailTool: a.mailTool || DEFAULT_MAIL_TOOL });
      onClose();
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };

  const s = a.schedule;
  const agents = fromBlocks ? fromBlocks.agents : Object.keys(a.agents);
  const days = s.kind === "daily" ? s.days : [];

  // A portal: inside a canvas node, `fixed` would follow the canvas zoom.
  return createPortal(
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-6"
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t(initial.id ? "auto.form.titleEdit" : "auto.form.titleNew")}
        className="flex max-h-full w-full max-w-xl flex-col rounded-2xl border border-border bg-surface-raised shadow-card"
      >
        <header className="flex items-center gap-2 border-b border-border px-5 py-3.5">
          <h2 className="font-serif text-lg font-medium">
            {t(initial.id ? "auto.form.titleEdit" : "auto.form.titleNew")}
          </h2>
          <IconButton label={t("auto.form.cancel")} onClick={onClose} className="ml-auto">
            <X size={17} />
          </IconButton>
        </header>
        <div className="min-h-0 flex-1 space-y-3.5 overflow-y-auto px-5 py-4">
          <Field label={t("auto.form.name")}>
            <input
              className={input}
              value={a.name}
              placeholder={t("auto.form.namePlaceholder")}
              onChange={(e) => patch({ name: e.target.value })}
            />
          </Field>
          <Field label={t("auto.form.prompt")}>
            <textarea
              className={`${input} min-h-24 resize-y`}
              value={a.prompt}
              placeholder={t("auto.form.promptPlaceholder")}
              onChange={(e) => patch({ prompt: e.target.value })}
            />
          </Field>

          <Field label={t("auto.form.when")}>
            <div className="space-y-2 rounded-lg border border-border p-2.5">
              <Segmented
                label={t("auto.form.when")}
                value={s.kind}
                options={[
                  { value: "daily", label: t("auto.form.daily") },
                  { value: "interval", label: t("auto.form.interval") },
                ]}
                onChange={(kind) =>
                  patch({
                    schedule:
                      kind === "daily"
                        ? { kind: "daily", time: "10:00", days: [] }
                        : { kind: "interval", minutes: 60 },
                  })
                }
              />
              {s.kind === "daily" ? (
                <div className="flex flex-wrap items-center gap-2">
                  <input
                    type="time"
                    aria-label={t("auto.form.time")}
                    className={box}
                    value={s.time}
                    onChange={(e) => patch({ schedule: { ...s, time: e.target.value } })}
                  />
                  <fieldset
                    className="flex gap-1"
                    aria-label={t("auto.form.days")}
                    title={t("auto.form.days")}
                  >
                    {[1, 2, 3, 4, 5, 6, 0].map((d) => {
                      const on = days.includes(d);
                      return (
                        <button
                          key={d}
                          type="button"
                          aria-pressed={on}
                          onClick={() =>
                            patch({
                              schedule: {
                                ...s,
                                days: on ? days.filter((x) => x !== d) : [...days, d],
                              },
                            })
                          }
                          className={`size-7 rounded-full text-xs transition-colors ${
                            on
                              ? "bg-accent text-accent-fg"
                              : "border border-border text-text-muted hover:bg-hover"
                          }`}
                        >
                          {dayLabel(d)}
                        </button>
                      );
                    })}
                  </fieldset>
                </div>
              ) : (
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="number"
                    min={5}
                    step={5}
                    className={`${box} w-24`}
                    value={s.minutes}
                    onChange={(e) =>
                      patch({ schedule: { kind: "interval", minutes: Number(e.target.value) } })
                    }
                  />
                  {t("auto.form.minutes")}
                </label>
              )}
            </div>
          </Field>

          <Field label={t("auto.form.model")}>
            <select
              className={input}
              value={a.model}
              onChange={(e) => patch({ model: e.target.value })}
            >
              <option value="">{t("auto.form.modelDefault")}</option>
              {MODELS.map((m) => (
                <option key={m.alias} value={m.alias}>
                  {m.label}
                </option>
              ))}
            </select>
          </Field>

          <Field label={t("auto.form.tools")} hint={t("auto.form.toolsHint")}>
            <div className="mb-2 flex flex-wrap gap-1.5">
              {TOOL_GROUPS.map((g) => {
                const on = groupOn(g.tools);
                return (
                  <button
                    key={g.key}
                    type="button"
                    aria-pressed={on}
                    title={g.tools.join(", ")}
                    onClick={() => toggleGroup(g.tools)}
                    className={`rounded-full px-2.5 py-1 text-xs transition-colors ${
                      on
                        ? "bg-accent-soft text-accent ring-1 ring-accent"
                        : "border border-border text-text-muted hover:bg-hover"
                    }`}
                  >
                    {t(g.label)}
                  </button>
                );
              })}
            </div>
            <input
              className={input}
              value={extra}
              placeholder={t("auto.form.extraTools")}
              onChange={(e) => setExtra(e.target.value)}
            />
          </Field>

          <Field label={t("auto.form.email")} hint={t("auto.form.mailHint")}>
            <div className="flex items-center gap-2">
              <Mail size={15} className="shrink-0 text-text-subtle" />
              <input
                type="email"
                className={input}
                value={a.email}
                placeholder={t("auto.form.emailPlaceholder")}
                onChange={(e) => patch({ email: e.target.value })}
              />
            </div>
            {a.email.trim() && (
              <input
                className={`${input} mt-2 font-mono text-xs`}
                value={a.mailTool}
                aria-label={t("auto.form.mailTool")}
                title={t("auto.form.mailTool")}
                onChange={(e) => patch({ mailTool: e.target.value })}
              />
            )}
          </Field>

          <Field label={t("auto.form.project")}>
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-sm text-text-muted" title={a.project}>
                {a.project || t("auto.form.projectNone")}
              </span>
              {a.project && (
                <Button size="sm" variant="ghost" onClick={() => patch({ project: "" })}>
                  {t("auto.form.clear")}
                </Button>
              )}
              <Button size="sm" onClick={() => void pickFolder()}>
                <FolderOpen size={14} />
                {t("auto.form.pick")}
              </Button>
            </div>
          </Field>

          {(fromBlocks || agents.length > 0 || a.instructions) && (
            <div className="rounded-lg bg-sunken px-3 py-2 text-xs text-text-muted">
              <div className="flex items-center gap-1.5">
                <Bot size={13} />
                {t("auto.form.agents")}: {agents.length ? agents.join(", ") : t("auto.form.none")}
              </div>
              {fromBlocks ? (
                <p className="mt-1 text-text-subtle">
                  {t("auto.form.fromBlocks", {
                    n: fromBlocks.instructions,
                    agents: agents.length ? agents.join(", ") : t("auto.form.none"),
                  })}
                </p>
              ) : (
                a.instructions && (
                  <p className="mt-1 line-clamp-3 text-text-subtle">
                    {t("auto.form.instructions")}: {a.instructions}
                  </p>
                )
              )}
            </div>
          )}

          {error && <p className="selectable text-xs whitespace-pre-wrap text-danger">{error}</p>}
        </div>
        <footer className="flex items-center justify-end gap-2 border-t border-border px-5 py-3">
          <Button variant="ghost" onClick={onClose}>
            {t("auto.form.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={busy || !a.name.trim() || !a.prompt.trim()}
            onClick={() => void submit()}
          >
            {t("auto.form.save")}
          </Button>
        </footer>
      </div>
    </div>,
    document.body,
  );
}
