import { type MessageKey, useT } from "../i18n";
import { type DockSide, type DockStatus, type TargetApp, useDockStore } from "../store/dock";

type T = ReturnType<typeof useT>;

function appName(target: TargetApp, t: T): string {
  switch (target) {
    case "windowsTerminal":
      return "Windows Terminal";
    case "console":
      return t("shell.dock.console");
    case "vsCode":
      return "VS Code";
    case "claudeDesktop":
      return t("shell.dock.claudeDesktop");
  }
}

const SIDES: Record<DockSide, MessageKey> = {
  right: "shell.dock.right",
  left: "shell.dock.left",
  inside: "shell.dock.inside",
};

function describe(status: DockStatus | null, t: T): string {
  if (!status) return t("shell.dock.connecting");
  switch (status.mode) {
    case "docked": {
      const app = status.target ? appName(status.target, t) : t("shell.dock.window");
      const where = t(status.side ? SIDES[status.side] : "shell.dock.waiting", { app });
      return status.magnet === "locked" ? `${where} · ${t("shell.dock.locked")}` : where;
    }
    case "edge":
      return t(status.magnet === "locked" ? "shell.dock.lockNext" : "shell.dock.edge");
    case "floating":
      return t("shell.dock.floating");
  }
}

export function StatusBar() {
  const status = useDockStore((s) => s.status);
  const attached = status?.mode === "docked";
  const t = useT();

  return (
    <footer className="flex h-7 shrink-0 items-center justify-between border-t border-border bg-surface-raised px-3 text-[11px] text-text-muted">
      <span className="flex items-center gap-1.5">
        <span
          className={`inline-block size-1.5 rounded-full ${attached ? "bg-accent" : "bg-text-muted"}`}
        />
        {describe(status, t)}
      </span>
      <span>Ctrl+Alt+Space</span>
    </footer>
  );
}
