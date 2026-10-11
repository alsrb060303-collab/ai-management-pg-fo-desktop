import { invoke } from "@tauri-apps/api/core";
import { diffLines } from "diff";
import { ChevronDown, ChevronRight, FolderOpen, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, IconButton } from "../../components/ui";
import { type MessageKey, useT } from "../../i18n";
import type { ComposerDoc } from "../../store/composer";
import { useProjectStore } from "../../store/project";

// Mirrors `PlanItem` / `Status` in src-tauri/src/composer/write.rs.
type Status =
  | { kind: "new" | "update" | "unchanged" | "modified" | "exists" }
  | { kind: "blocked"; reason: string };

interface PlanItem {
  path: string;
  status: Status;
  personal: boolean;
  old: string | null;
  new: string;
  oldHash: string | null;
  note: string | null;
}

interface WriteResult {
  written: string[];
  skipped: [string, string][];
  warnings: [string, string][];
}

const STATUS: Record<Status["kind"], { label: MessageKey; className: string }> = {
  new: { label: "composer.write.status.new", className: "text-success" },
  update: { label: "composer.write.status.update", className: "text-info" },
  unchanged: { label: "composer.write.status.unchanged", className: "text-text-subtle" },
  modified: { label: "composer.write.status.modified", className: "text-warning" },
  exists: { label: "composer.write.status.exists", className: "text-warning" },
  blocked: { label: "composer.write.status.blocked", className: "text-danger" },
};

const needsConfirm = (s: Status) => s.kind === "modified" || s.kind === "exists";

export function WriteDialog({ doc, onClose }: { doc: ComposerDoc; onClose: () => void }) {
  const { current, choose, refresh } = useProjectStore();
  const [plan, setPlan] = useState<PlanItem[] | null>(null);
  const [confirmed, setConfirmed] = useState<Set<string>>(new Set());
  const [open, setOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<WriteResult | null>(null);
  const t = useT();

  const load = useCallback(async () => {
    if (!current) return;
    setPlan(null);
    setError(null);
    // Confirmations belong to the files shown for one project.
    setConfirmed(new Set());
    setOpen(null);
    try {
      setPlan(await invoke<PlanItem[]>("composer_plan", { project: current, doc }));
    } catch (e) {
      setError(String(e));
    }
  }, [current, doc]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const writable =
    plan?.filter(
      (p) =>
        p.status.kind === "new" ||
        p.status.kind === "update" ||
        (needsConfirm(p.status) && confirmed.has(p.path)),
    ) ?? [];

  const write = async () => {
    if (!current) return;
    setBusy(true);
    setError(null);
    try {
      // Each confirmation is tied to the content shown, so a file that changed
      // since the preview is not overwritten.
      const confirmations = (plan ?? [])
        .filter((p) => confirmed.has(p.path))
        .map((p) => ({ path: p.path, oldHash: p.oldHash }));
      const res = await invoke<WriteResult>("composer_write", {
        project: current,
        doc,
        confirmed: confirmations,
      });
      setResult(res);
      void refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-6"
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("composer.write.title")}
        className="flex max-h-full w-full max-w-2xl flex-col rounded-2xl border border-border bg-surface-raised shadow-card"
      >
        <header className="flex items-center gap-2 border-b border-border px-5 py-3.5">
          <h2 className="font-serif text-lg font-medium">{t("composer.write.title")}</h2>
          <IconButton label={t("composer.write.close")} onClick={onClose} className="ml-auto">
            <X size={17} />
          </IconButton>
        </header>

        <div className="flex items-center gap-2 border-b border-border px-5 py-3 text-sm">
          <FolderOpen size={15} className="shrink-0 text-text-subtle" />
          <span className="selectable min-w-0 flex-1 truncate" title={current ?? undefined}>
            {current ?? t("composer.write.pickProject")}
          </span>
          <Button size="sm" onClick={() => void choose()}>
            {current ? t("composer.write.change") : t("composer.write.chooseFolder")}
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">
          {error && (
            <p className="mb-3 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">{error}</p>
          )}
          {result ? (
            <Result result={result} />
          ) : !current ? null : !plan ? (
            <p className="text-sm text-text-muted">{t("composer.write.checking")}</p>
          ) : (
            <ul className="space-y-1.5">
              {plan.map((item) => {
                const status = STATUS[item.status.kind];
                const expanded = open === item.path;
                return (
                  <li key={item.path} className="rounded-lg border border-border">
                    <div className="flex items-center gap-2 px-3 py-2">
                      <button
                        type="button"
                        aria-label={
                          expanded
                            ? t("composer.write.hideChanges")
                            : t("composer.write.showChanges")
                        }
                        onClick={() => setOpen(expanded ? null : item.path)}
                        className="text-text-subtle hover:text-text"
                      >
                        {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
                      </button>
                      <span className="min-w-0 flex-1 truncate font-mono text-[12px]">
                        {item.path}
                      </span>
                      {item.personal && (
                        <span className="rounded bg-sunken px-1 text-[10px] text-text-subtle">
                          {t("composer.personal")}
                        </span>
                      )}
                      <span className={`shrink-0 text-xs font-medium ${status.className}`}>
                        {t(status.label)}
                      </span>
                    </div>
                    {item.note && <p className="px-9 pb-2 text-xs text-info">{item.note}</p>}
                    {item.status.kind === "blocked" && (
                      <p className="px-9 pb-2 text-xs text-danger">{item.status.reason}</p>
                    )}
                    {needsConfirm(item.status) && (
                      <label className="flex items-center gap-2 px-9 pb-2 text-xs text-text-muted">
                        <input
                          type="checkbox"
                          checked={confirmed.has(item.path)}
                          onChange={(e) => {
                            const next = new Set(confirmed);
                            if (e.target.checked) next.add(item.path);
                            else next.delete(item.path);
                            setConfirmed(next);
                          }}
                        />
                        {item.status.kind === "modified"
                          ? t("composer.write.overwriteModified")
                          : t("composer.write.overwriteForeign")}
                      </label>
                    )}
                    {expanded && <Diff old={item.old ?? ""} next={item.new} />}
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <footer className="flex items-center gap-2 border-t border-border px-5 py-3">
          {result ? (
            <Button variant="primary" className="ml-auto" onClick={onClose}>
              {t("composer.write.close")}
            </Button>
          ) : (
            <>
              <span className="text-xs text-text-subtle">{t("composer.write.hint")}</span>
              <Button variant="ghost" className="ml-auto" onClick={onClose}>
                {t("composer.write.cancel")}
              </Button>
              <Button
                variant="primary"
                disabled={!current || busy || writable.length === 0}
                onClick={() => void write()}
              >
                {busy
                  ? t("composer.write.writing")
                  : t("composer.write.writeFiles", { n: writable.length })}
              </Button>
            </>
          )}
        </footer>
      </div>
    </div>
  );
}

function Result({ result }: { result: WriteResult }) {
  const t = useT();
  return (
    <div className="space-y-3 text-sm">
      <p>
        {result.written.length > 0
          ? t("composer.write.wrote", { n: result.written.length })
          : t("composer.write.wroteNone")}
      </p>
      {result.written.length > 0 && (
        <ul className="space-y-0.5 font-mono text-[12px] text-success">
          {result.written.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
      {result.warnings.length > 0 && (
        <ul className="space-y-0.5 text-xs text-warning">
          {result.warnings.map(([p, why]) => (
            <li key={p}>
              <span className="font-mono">{p}</span>:{" "}
              {t("composer.write.gitExcludeFailed", { reason: why })}
            </li>
          ))}
        </ul>
      )}
      {result.skipped.length > 0 && (
        <ul className="space-y-0.5 text-xs text-warning">
          {result.skipped.map(([p, why]) => (
            <li key={p}>
              <span className="font-mono">{p}</span>: {why}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Diff({ old, next }: { old: string; next: string }) {
  const parts = diffLines(old, next);
  return (
    <pre className="selectable max-h-72 overflow-auto border-t border-border bg-sunken px-3 py-2 font-mono text-[11.5px] leading-[1.55]">
      {parts.map((part, i) => {
        const cls = part.added
          ? "bg-success/15 text-success"
          : part.removed
            ? "bg-danger-soft text-danger line-through decoration-danger/40"
            : "text-text-muted";
        const sign = part.added ? "+ " : part.removed ? "- " : "  ";
        const lines = part.value.replace(/\n$/, "").split("\n");
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: diff hunks have no identity besides position
          <span key={i} className={`block ${cls}`}>
            {lines.map((l) => `${sign}${l}`).join("\n")}
          </span>
        );
      })}
    </pre>
  );
}
