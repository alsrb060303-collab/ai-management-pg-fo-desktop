import { useDockStore } from "../store/dock";

// The window is undecorated, so this bar provides dragging and the window buttons.
export function TitleBar() {
  const status = useDockStore((s) => s.status);
  const setEnabled = useDockStore((s) => s.setEnabled);
  const setPinned = useDockStore((s) => s.setPinned);
  const hide = useDockStore((s) => s.hide);

  const docked = status?.enabled ?? false;
  const pinned = status?.pinned ?? false;

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
          label={docked ? "터미널에서 떼기" : "터미널 옆에 붙이기"}
          onClick={() => setEnabled(!docked)}
          active={docked}
        >
          <MagnetIcon />
        </TitleButton>
        <TitleButton
          label={pinned ? "항상 위 해제" : "항상 위"}
          onClick={() => setPinned(!pinned)}
          active={pinned}
        >
          <PinIcon />
        </TitleButton>
        <TitleButton label="숨기기 (Ctrl+Alt+Space)" onClick={hide} danger>
          <CloseIcon />
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
        danger ? "hover:bg-red-600 hover:text-white" : "hover:bg-border"
      } ${active ? "text-accent" : "text-text-muted"}`}
    >
      {children}
    </button>
  );
}

const iconProps = {
  width: 14,
  height: 14,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

function MagnetIcon() {
  return (
    <svg aria-hidden="true" {...iconProps}>
      <path d="M6 3v8a6 6 0 0 0 12 0V3" />
      <path d="M6 7h4M14 7h4" />
    </svg>
  );
}

function PinIcon() {
  return (
    <svg aria-hidden="true" {...iconProps}>
      <path d="M12 17v5" />
      <path d="M9 3h6l-1 6 3 4H7l3-4z" />
    </svg>
  );
}

function CloseIcon() {
  return (
    <svg aria-hidden="true" {...iconProps}>
      <path d="M6 6l12 12M18 6L6 18" />
    </svg>
  );
}
