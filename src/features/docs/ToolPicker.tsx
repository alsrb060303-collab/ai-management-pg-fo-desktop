import { useT } from "../../i18n";
import { useProjectStore } from "../../store/project";
import { TOOL_IDS, TOOLS, type ToolPref } from "../../tools";

/** Which AI tool the current project is handled as: detected, or picked. */
export function ToolPicker() {
  const t = useT();
  const { toolPref, detection, loading, setToolPref } = useProjectStore();
  const detected = detection ? TOOLS[detection.tool].name : "…";

  let reason = "";
  if (toolPref !== "auto") {
    reason = t("tools.picked", { tool: detected });
  } else if (detection?.reason === "files") {
    const files = detection.signals
      .filter((s) => s.tool === detection.tool)
      .map((s) => s.path)
      .join(", ");
    reason = t("tools.reason.files", { files });
  } else if (detection?.reason === "installed") {
    reason = t("tools.reason.installed", { tool: detected });
  } else if (detection) {
    reason = t("tools.reason.fallback");
  }

  return (
    <div className="flex items-center gap-1.5 text-xs">
      <label htmlFor="tool-picker" className="shrink-0 text-text-muted">
        {t("tools.label")}
      </label>
      <select
        id="tool-picker"
        value={toolPref}
        disabled={loading && !detection}
        onChange={(e) => void setToolPref(e.target.value as ToolPref)}
        className="shrink-0 rounded-md border border-border bg-surface-raised px-2 py-1"
      >
        <option value="auto">{t("tools.autoWith", { tool: detected })}</option>
        {TOOL_IDS.map((id) => (
          <option key={id} value={id}>
            {TOOLS[id].name}
          </option>
        ))}
      </select>
      <span className="min-w-0 truncate text-[11px] text-text-muted" title={reason}>
        {reason}
      </span>
    </div>
  );
}
