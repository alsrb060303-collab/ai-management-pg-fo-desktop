import { AppWindow, EyeOff, Magnet, Pin } from "lucide-react";
import { Logo } from "../components/Logo";
import { useT } from "../i18n";
import { useDockStore } from "../store/dock";
import { switchView } from "../store/view";

// The window is undecorated, so this bar provides dragging and the window buttons.
export function TitleBar() {
  const status = useDockStore((s) => s.status);
  const setEnabled = useDockStore((s) => s.setEnabled);
  const setPinned = useDockStore((s) => s.setPinned);
  const hide = useDockStore((s) => s.hide);
  const t = useT();

  const docked = status?.enabled ?? false;
  const pinned = status?.pinned ?? false;

  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center justify-between border-b border-border bg-surface-raised pl-3"
    >
      <span
        data-tauri-drag-region
        className="flex items-center gap-1.5 text-xs font-semibold tracking-wide text-text-muted"
      >
        <Logo size={15} className="pointer-events-none" />
        AI Management
      </span>
      <div className="flex h-full">
        <TitleButton label={t("shell.title.toMain")} onClick={() => void switchView("main")}>
          <AppWindow size={14} />
        </TitleButton>
        <TitleButton
          label={docked ? t("shell.title.undock") : t("shell.title.dock")}
          onClick={() => setEnabled(!docked)}
          active={docked}
        >
          <Magnet size={14} />
        </TitleButton>
        <TitleButton
          label={pinned ? t("shell.title.unpin") : t("shell.title.pin")}
          onClick={() => setPinned(!pinned)}
          active={pinned}
        >
          <Pin size={14} />
        </TitleButton>
        <TitleButton label={t("shell.title.hide")} onClick={hide} danger>
          <EyeOff size={14} />
        </TitleButton>
      </div>
    </header>
  );
}

interface TitleButtonProps {
  label: string;
  onClick: () => void;
  active?: boolean;
  danger?: boolean;
  children: React.ReactNode;
}

function TitleButton({ label, onClick, active, danger, children }: TitleButtonProps) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active}
      onClick={onClick}
      className={`flex h-full w-9 items-center justify-center transition-colors ${
        danger ? "hover:bg-danger-soft hover:text-danger" : "hover:bg-hover"
      } ${active ? "text-accent" : "text-text-muted"}`}
    >
      {children}
    </button>
  );
}
