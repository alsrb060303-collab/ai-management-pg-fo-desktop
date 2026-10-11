import {
  Blocks,
  FolderOpen,
  Library,
  type LucideIcon,
  Map as MapIcon,
  PanelRightOpen,
} from "lucide-react";
import { Logo } from "../../components/Logo";
import { useProjectStore } from "../../store/project";
import { useWorkspaceStore } from "../../store/workspace";
import { switchToSidebar } from "../leave";

function greeting(hour: number): string {
  if (hour >= 5 && hour < 12) return "좋은 아침이에요";
  if (hour >= 12 && hour < 18) return "좋은 오후예요";
  return "좋은 저녁이에요";
}

export function HomePage() {
  const go = useWorkspaceStore((s) => s.go);
  const { recent, select } = useProjectStore();

  const actions: { icon: LucideIcon; title: string; description: string; onClick: () => void }[] = [
    {
      icon: Blocks,
      title: "새 CLAUDE.md 만들기",
      description: "추천 블록을 연결해서 지침 · 규칙 · skill · agent 문서를 만들어요.",
      onClick: () => go("composer"),
    },
    {
      icon: MapIcon,
      title: "프로젝트 설정 보기",
      description: "Claude Code가 이 프로젝트에서 읽는 설정을 범위별로 확인해요.",
      onClick: () => go("config"),
    },
    {
      icon: Library,
      title: "라이브러리",
      description: "내 skill · subagent를 프로젝트에 적용하거나 빼요.",
      onClick: () => go("library"),
    },
    {
      icon: PanelRightOpen,
      title: "사이드바로 전환",
      description: "터미널 옆에 붙여서 작업하면서 봐요.",
      onClick: () => void switchToSidebar(),
    },
  ];

  return (
    <div className="mx-auto flex min-h-full max-w-2xl flex-col justify-center px-8 py-16">
      <div className="mb-2 flex items-center gap-3">
        <Logo size={40} className="shrink-0 drop-shadow-sm" />
        <h1 className="font-serif text-[32px] leading-tight font-medium tracking-tight">
          {greeting(new Date().getHours())}
        </h1>
      </div>
      <p className="mb-8 text-text-muted">오늘은 어떤 문서를 만들어 볼까요?</p>

      <div className="grid grid-cols-2 gap-3">
        {actions.map(({ icon: Icon, title, description, onClick }) => (
          <button
            key={title}
            type="button"
            onClick={onClick}
            className="group rounded-xl border border-border bg-surface-raised p-4 text-left shadow-card transition-colors hover:border-border-strong"
          >
            <Icon
              size={18}
              className="mb-3 text-text-muted transition-colors group-hover:text-accent"
            />
            <div className="text-sm font-medium">{title}</div>
            <div className="mt-1 text-[13px] leading-snug text-text-muted">{description}</div>
          </button>
        ))}
      </div>

      {recent.length > 0 && (
        <section className="mt-10">
          <h2 className="mb-2 px-1 text-xs font-medium text-text-subtle">최근 프로젝트</h2>
          <ul className="space-y-0.5">
            {recent.slice(0, 5).map((path) => (
              <li key={path}>
                <button
                  type="button"
                  onClick={() => {
                    void select(path);
                    go("config");
                  }}
                  className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-hover"
                >
                  <FolderOpen size={15} className="shrink-0 text-text-subtle" />
                  <span className="max-w-[45%] shrink-0 truncate">{baseName(path)}</span>
                  <span className="min-w-0 flex-1 truncate text-right text-xs text-text-subtle">
                    {path}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
