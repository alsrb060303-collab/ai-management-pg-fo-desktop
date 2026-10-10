import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useUiStore } from "../store/ui";

// The window is undecorated, so this bar provides dragging and the window buttons.
export function TitleBar() {
  const pinned = useUiStore((s) => s.pinned);
  const setPinned = useUiStore((s) => s.setPinned);

  const togglePin = async () => {
    const next = !pinned;
    if (isTauri()) await getCurrentWindow().setAlwaysOnTop(next);
    setPinned(next);
  };

  const minimize = () => {
    if (isTauri()) void getCurrentWindow().minimize();
  };

  const close = () => {
    if (isTauri()) void getCurrentWindow().close();
  };

  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center justify-between border-b border-border bg-surface-raised pl-3"
    >
      <span data-tauri-drag-region className="text-xs font-semibold tracking-wide text-text-muted">
        AI Management
      </span>
      <div className="flex h-full">
        <TitleButton
          label={pinned ? "항상 위 해제" : "항상 위"}
          onClick={togglePin}
          active={pinned}
        >
          📌
        </TitleButton>
        <TitleButton label="최소화" onClick={minimize}>
          —
        </TitleButton>
        <TitleButton label="닫기" onClick={close} danger>
          ✕
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
      className={`flex h-full w-9 items-center justify-center text-xs transition-colors ${
        danger ? "hover:bg-red-600 hover:text-white" : "hover:bg-border"
      } ${active ? "text-accent" : "text-text-muted"}`}
    >
      {children}
    </button>
  );
}
