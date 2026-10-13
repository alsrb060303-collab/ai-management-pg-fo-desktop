import { useEffect } from "react";
import { type MessageKey, useT } from "../../i18n";
import { type ApplyStatus, type LibraryItem, useLibraryStore } from "../../store/library";
import { useProjectStore } from "../../store/project";
import { ModeButtons, smallButton } from "./ModeButtons";

const STATUS_TEXT: Record<ApplyStatus, { label: MessageKey; className: string }> = {
  current: { label: "docs.status.current", className: "text-success" },
  outdated: { label: "docs.status.outdated", className: "text-info" },
  modified: { label: "docs.status.modified", className: "text-warning" },
  missing: { label: "docs.status.missing", className: "text-warning" },
  conflict: { label: "docs.status.conflict", className: "text-text-muted" },
};

export function LibraryPanel() {
  const t = useT();
  const current = useProjectStore((s) => s.current);
  const { overview, busy, error, load, chooseDir, useDefaultDir } = useLibraryStore();

  // Reload whenever the selected project changes (statuses are per project).
  useEffect(() => {
    void current;
    void load();
  }, [current, load]);

  const skills = overview?.items.filter((i) => i.kind === "skill") ?? [];
  const agents = overview?.items.filter((i) => i.kind === "agent") ?? [];

  return (
    <div className="space-y-3">
      <div className="rounded-md border border-border bg-surface-raised px-2.5 py-2 text-xs">
        <div className="flex items-center gap-1.5">
          <span className="min-w-0 flex-1 truncate text-text-muted" title={overview?.dir}>
            {overview ? overview.dir : "…"}
          </span>
          <button type="button" onClick={chooseDir} className={smallButton} disabled={busy}>
            {t("docs.library.changeFolder")}
          </button>
          {overview && !overview.isDefault && (
            <button type="button" onClick={useDefaultDir} className={smallButton} disabled={busy}>
              {t("docs.library.default")}
            </button>
          )}
        </div>
        <p className="mt-1 text-fs-10 text-text-muted">{t("docs.library.structureHint")}</p>
      </div>

      {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-xs text-danger">{error}</p>}
      {!current && <p className="text-xs text-text-muted">{t("docs.library.pickProject")}</p>}

      <Section title={t("docs.section.skill")} items={skills} />
      <Section title={t("docs.section.agent")} items={agents} />
      {overview && overview.items.length === 0 && (
        <p className="text-xs text-text-muted">{t("docs.library.empty")}</p>
      )}
    </div>
  );
}

function Section({ title, items }: { title: string; items: LibraryItem[] }) {
  if (items.length === 0) return null;
  return (
    <section>
      <h3 className="mb-1.5 text-xs font-semibold">
        {title} <span className="font-normal text-text-muted">· {items.length}</span>
      </h3>
      <ul className="space-y-1.5">
        {items.map((item) => (
          <LibraryRow key={`${item.kind}:${item.id}`} item={item} />
        ))}
      </ul>
    </section>
  );
}

function LibraryRow({ item }: { item: LibraryItem }) {
  const t = useT();
  const current = useProjectStore((s) => s.current);
  const { busy, apply, remove, update, forget } = useLibraryStore();
  const status = item.status ? STATUS_TEXT[item.status] : null;
  const applied = item.status !== null && item.status !== "conflict";

  return (
    <li className="rounded-md border border-border bg-surface-raised px-2.5 py-1.5">
      <div className="flex items-center gap-1.5 text-xs">
        <span className="min-w-0 flex-1 truncate font-medium" title={item.path}>
          {item.name}
        </span>
        {item.mode && (
          <span className="shrink-0 rounded bg-sunken px-1 text-fs-10 text-text-muted">
            {t(item.mode === "personal" ? "docs.mode.personal" : "docs.mode.team")}
          </span>
        )}
        {status && (
          <span className={`shrink-0 text-fs-10 ${status.className}`}>{t(status.label)}</span>
        )}
      </div>
      {item.description && (
        <p className="mt-0.5 line-clamp-2 text-fs-11 text-text-muted">{item.description}</p>
      )}
      {current && (
        <div className="mt-1.5 flex flex-wrap gap-1">
          {item.status === null && (
            <ModeButtons
              label={t("docs.library.apply")}
              disabled={busy}
              onPick={(mode) => apply(item, mode)}
            />
          )}
          {item.status === "outdated" && (
            <button
              type="button"
              className={smallButton}
              disabled={busy}
              onClick={() => update(item)}
            >
              {t("docs.library.update")}
            </button>
          )}
          {item.status === "modified" && (
            <button
              type="button"
              className={smallButton}
              disabled={busy}
              onClick={() => update(item)}
            >
              {t("docs.library.overwrite")}
            </button>
          )}
          {applied && (
            <button
              type="button"
              className={smallButton}
              disabled={busy}
              onClick={() => (item.status === "missing" ? forget(item) : remove(item))}
            >
              {t(item.status === "missing" ? "docs.library.forget" : "docs.library.remove")}
            </button>
          )}
        </div>
      )}
    </li>
  );
}
