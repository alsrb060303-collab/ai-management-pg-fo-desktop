import { ask } from "@tauri-apps/plugin-dialog";
import {
  Blocks,
  GitBranch,
  House,
  Library,
  type LucideIcon,
  Map as MapIcon,
  NotebookPen,
  PanelLeft,
  PanelRightOpen,
  Plus,
  Settings,
  Trash2,
} from "lucide-react";
import { Logo } from "../components/Logo";
import { IconButton } from "../components/ui";
import { useComposerStore } from "../store/composer";
import { type Page, useWorkspaceStore } from "../store/workspace";
import { switchToSidebar } from "./leave";

const NAV: { page: Page; label: string; icon: LucideIcon; soon?: boolean }[] = [
  { page: "home", label: "홈", icon: House },
  { page: "composer", label: "문서 작성기", icon: Blocks },
  { page: "library", label: "라이브러리", icon: Library },
  { page: "config", label: "설정 지도", icon: MapIcon },
  { page: "work", label: "작업", icon: GitBranch },
  { page: "memo", label: "메모", icon: NotebookPen },
];

export function NavSidebar() {
  const { page, collapsed, go, toggleCollapsed } = useWorkspaceStore();
  const docs = useComposerStore((s) => s.docs);
  const openId = useComposerStore((s) => s.doc?.id);
  const { newDoc, openDoc, deleteDoc } = useComposerStore.getState();

  return (
    <aside
      className={`flex shrink-0 flex-col border-r border-border bg-sidebar transition-[width] duration-200 ${
        collapsed ? "w-[52px]" : "w-64"
      }`}
    >
      <div className="flex h-12 shrink-0 items-center gap-1 px-2.5">
        {!collapsed && (
          <button
            type="button"
            onClick={() => go("home")}
            className="flex min-w-0 items-center gap-2 rounded-md px-1.5 font-serif text-[17px] font-semibold tracking-tight"
          >
            <Logo size={20} />
            <span className="truncate">AI Management</span>
          </button>
        )}
        <IconButton
          label={collapsed ? "메뉴 펼치기" : "메뉴 접기"}
          onClick={toggleCollapsed}
          className={collapsed ? "" : "ml-auto"}
        >
          <PanelLeft size={18} />
        </IconButton>
      </div>

      <div className="px-2">
        <NavButton
          icon={Plus}
          label="새 문서"
          collapsed={collapsed}
          accent
          onClick={() => {
            newDoc();
            go("composer");
          }}
        />
      </div>

      <nav className="mt-2 space-y-0.5 px-2">
        {NAV.map((item) => (
          <NavButton
            key={item.page}
            icon={item.icon}
            label={item.label}
            collapsed={collapsed}
            active={page === item.page}
            badge={item.soon ? "준비 중" : undefined}
            onClick={() => go(item.page)}
          />
        ))}
      </nav>

      <div className="mt-5 min-h-0 flex-1 overflow-y-auto px-2">
        {!collapsed && (
          <>
            <h3 className="px-2 pb-1 text-xs font-medium text-text-subtle">최근 문서</h3>
            {docs.length === 0 ? (
              <p className="px-2 text-xs text-text-subtle">아직 만든 문서가 없어요.</p>
            ) : (
              <ul className="space-y-px">
                {docs.slice(0, 20).map((d) => {
                  const active = page === "composer" && d.id === openId;
                  return (
                    <li key={d.id} className="group relative">
                      <button
                        type="button"
                        onClick={() => {
                          void openDoc(d.id);
                          go("composer");
                        }}
                        className={`w-full truncate rounded-lg py-1.5 pr-8 pl-2 text-left text-[13px] transition-colors ${
                          active
                            ? "bg-active text-text"
                            : "text-text-muted hover:bg-hover hover:text-text"
                        }`}
                      >
                        {d.name}
                      </button>
                      <button
                        type="button"
                        aria-label={`${d.name} 삭제`}
                        title="삭제"
                        onClick={async () => {
                          const ok = await ask(
                            `'${d.name}' 문서를 삭제할까요? 프로젝트에 쓴 파일은 그대로 남아요.`,
                            {
                              title: "문서 삭제",
                              kind: "warning",
                            },
                          ).catch(() => false);
                          if (ok) void deleteDoc(d.id);
                        }}
                        className="absolute top-1/2 right-1 -translate-y-1/2 rounded-md p-1 text-text-subtle opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-danger focus-visible:opacity-100"
                      >
                        <Trash2 size={13} />
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </>
        )}
      </div>

      <div className="space-y-0.5 border-t border-border p-2">
        <NavButton
          icon={PanelRightOpen}
          label="사이드바로 전환"
          collapsed={collapsed}
          onClick={() => void switchToSidebar()}
        />
        <NavButton
          icon={Settings}
          label="설정"
          collapsed={collapsed}
          active={page === "settings"}
          onClick={() => go("settings")}
        />
      </div>
    </aside>
  );
}

interface NavButtonProps {
  icon: LucideIcon;
  label: string;
  collapsed: boolean;
  active?: boolean;
  accent?: boolean;
  badge?: string;
  onClick: () => void;
}

function NavButton({
  icon: Icon,
  label,
  collapsed,
  active,
  accent,
  badge,
  onClick,
}: NavButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={collapsed ? label : undefined}
      aria-label={collapsed ? label : undefined}
      aria-current={active ? "page" : undefined}
      className={`flex h-9 w-full items-center gap-2.5 rounded-lg px-2 text-sm transition-colors ${
        active
          ? "bg-active font-medium text-text"
          : "text-text-muted hover:bg-hover hover:text-text"
      }`}
    >
      <span
        className={`flex shrink-0 items-center justify-center ${
          accent ? "size-6 -ml-0.5 rounded-full bg-accent text-accent-fg" : "size-5"
        }`}
      >
        <Icon size={accent ? 15 : 17} strokeWidth={accent ? 2.5 : 2} />
      </span>
      {!collapsed && (
        <span className={`truncate ${accent ? "font-medium text-text" : ""}`}>{label}</span>
      )}
      {!collapsed && badge && (
        <span className="ml-auto shrink-0 rounded-full bg-sunken px-1.5 py-px text-[10px] text-text-subtle">
          {badge}
        </span>
      )}
    </button>
  );
}
