import { TriangleAlert, X } from "lucide-react";
import { useT } from "../../i18n";
import { useSyncStore } from "../../store/sync";

/** Conflicts and skipped items from syncing documents and the library. */
export function SyncNotice() {
  const t = useT();
  const notice = useSyncStore((s) => s.notice);
  const dismiss = useSyncStore((s) => s.dismissNotice);
  if (!notice) return null;
  return (
    <div
      role="alert"
      className="fixed right-4 bottom-4 z-50 w-96 max-w-[calc(100vw-2rem)] rounded-xl border border-warning/40 bg-surface-raised p-3.5 shadow-card"
    >
      <div className="flex items-start gap-2">
        <TriangleAlert size={16} className="mt-0.5 shrink-0 text-warning" />
        <div className="min-w-0 flex-1 space-y-1.5 text-xs">
          <p className="text-sm font-medium">{t("sync.notice.title")}</p>
          <ul className="max-h-48 space-y-1 overflow-y-auto text-text-muted">
            {notice.conflicts.map((c) => (
              <li key={`${c.kind}:${c.name}:${c.copy ?? ""}`}>
                {c.copy
                  ? t("sync.notice.copy", { name: c.name, copy: c.copy })
                  : t("sync.notice.revived", { name: c.name })}
              </li>
            ))}
            {notice.tooBig.length > 0 && (
              <li>{t("sync.notice.tooBig", { names: notice.tooBig.join(", ") })}</li>
            )}
          </ul>
        </div>
        <button
          type="button"
          aria-label={t("sync.notice.dismiss")}
          onClick={dismiss}
          className="shrink-0 rounded p-0.5 text-text-subtle hover:bg-hover hover:text-text"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
