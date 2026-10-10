import { type DockStatus, type TargetApp, useDockStore } from "../store/dock";

const APP_NAMES: Record<TargetApp, string> = {
  windowsTerminal: "Windows Terminal",
  console: "콘솔",
  vsCode: "VS Code",
  claudeDesktop: "Claude 데스크탑",
};

const SIDE_NAMES = { right: "오른쪽", left: "왼쪽", inside: "겹쳐서" } as const;

function describe(status: DockStatus | null): string {
  if (!status) return "연결 중…";
  switch (status.mode) {
    case "docked": {
      const app = status.target ? APP_NAMES[status.target] : "창";
      return status.side ? `${app} ${SIDE_NAMES[status.side]}에 붙음` : `${app} 대기 중`;
    }
    case "edge":
      return "화면 가장자리";
    case "floating":
      return "도킹 꺼짐";
  }
}

export function StatusBar() {
  const status = useDockStore((s) => s.status);
  const attached = status?.mode === "docked";

  return (
    <footer className="flex h-7 shrink-0 items-center justify-between border-t border-border bg-surface-raised px-3 text-[11px] text-text-muted">
      <span className="flex items-center gap-1.5">
        <span
          className={`inline-block size-1.5 rounded-full ${attached ? "bg-accent" : "bg-text-muted"}`}
        />
        {describe(status)}
      </span>
      <span>Ctrl+Alt+Space</span>
    </footer>
  );
}
