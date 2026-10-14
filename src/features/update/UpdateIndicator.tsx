import { Download } from "lucide-react";
import { useT } from "../../i18n";
import { useUpdaterStore } from "../../store/updater";

/**
 * What stays after the update card was closed with "later": a small item that
 * installs the update when clicked (the card already showed the details) and
 * shows the progress while it downloads. `variant` is the main window's left
 * nav (`nav`, icon only when `collapsed`) or the docked sidebar's status bar
 * (`bar`).
 */
export function UpdateIndicator({
  variant,
  collapsed = false,
}: {
  variant: "nav" | "bar";
  collapsed?: boolean;
}) {
  const t = useT();
  const { status, version, progress, dismissed, install } = useUpdaterStore();
  const downloading = status === "downloading";
  if (!version || (status !== "available" && !downloading) || dismissed !== version) return null;

  const percent = progress === null ? null : Math.round(progress * 100);
  const label = downloading
    ? percent === null
      ? t("update.downloadingShort")
      : t("update.downloadingPercent", { percent })
    : t("update.indicator");
  const title = downloading ? t("update.downloading") : t("update.indicatorTitle", { version });

  if (variant === "bar") {
    return (
      <button
        type="button"
        disabled={downloading}
        onClick={() => void install()}
        title={title}
        aria-label={title}
        className="flex items-center gap-1 rounded px-1 text-accent hover:bg-hover disabled:cursor-default disabled:hover:bg-transparent"
      >
        <Download size={12} className={downloading ? "animate-pulse" : ""} />
        {downloading && <span>{percent === null ? "…" : `${percent}%`}</span>}
      </button>
    );
  }

  return (
    <button
      type="button"
      disabled={downloading}
      onClick={() => void install()}
      title={collapsed ? title : undefined}
      aria-label={collapsed ? title : undefined}
      className="flex h-9 w-full items-center gap-2.5 rounded-lg px-2 text-sm text-accent transition-colors hover:bg-hover disabled:cursor-default disabled:hover:bg-transparent"
    >
      <span className="flex size-5 shrink-0 items-center justify-center">
        <Download size={17} className={downloading ? "animate-pulse" : ""} />
      </span>
      {!collapsed && <span className="truncate">{label}</span>}
    </button>
  );
}
