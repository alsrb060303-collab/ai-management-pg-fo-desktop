import { Download, X } from "lucide-react";
import { useT } from "../../i18n";
import { useUpdaterStore } from "../../store/updater";

/** "A new version is out", with a button that installs it and restarts. */
export function UpdateNotice({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const { status, version, progress, dismissed, install, dismiss } = useUpdaterStore();
  const downloading = status === "downloading";
  if (!version || (status !== "available" && !downloading) || dismissed === version) return null;

  return (
    <div
      role="status"
      className={`fixed z-50 rounded-xl border border-accent/40 bg-surface-raised p-3 shadow-card ${
        compact ? "right-2 bottom-9 left-2" : "bottom-4 left-4 w-80 max-w-[calc(100vw-2rem)]"
      }`}
    >
      <div className="flex items-start gap-2">
        <Download size={16} className="mt-0.5 shrink-0 text-accent" />
        <div className="min-w-0 flex-1 space-y-2 text-xs">
          <p className="text-sm font-medium">{t("update.available", { version })}</p>
          {downloading ? (
            <div className="space-y-1">
              <div className="h-1.5 overflow-hidden rounded-full bg-sunken">
                <div
                  className={`h-full rounded-full bg-accent transition-[width] ${
                    progress === null ? "w-1/3 animate-pulse" : ""
                  }`}
                  style={progress === null ? undefined : { width: `${progress * 100}%` }}
                />
              </div>
              <p className="text-text-muted">{t("update.downloading")}</p>
            </div>
          ) : (
            <button
              type="button"
              onClick={() => void install()}
              className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover"
            >
              {t("update.install")}
            </button>
          )}
        </div>
        {!downloading && (
          <button
            type="button"
            aria-label={t("update.later")}
            title={t("update.later")}
            onClick={dismiss}
            className="shrink-0 rounded p-0.5 text-text-subtle hover:bg-hover hover:text-text"
          >
            <X size={14} />
          </button>
        )}
      </div>
    </div>
  );
}
