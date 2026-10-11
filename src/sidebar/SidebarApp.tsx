import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { DocsTab } from "../features/docs/DocsTab";
import { MemoPanel } from "../features/memo/MemoPanel";
import { subscribeDockStatus } from "../store/dock";
import { useLibraryStore } from "../store/library";
import { restoreProject, useProjectStore } from "../store/project";
import { type TabId, useUiStore } from "../store/ui";
import type { ViewMode } from "../store/view";
import { StatusBar } from "./StatusBar";
import { TabNav } from "./TabNav";
import { TitleBar } from "./TitleBar";

// Each tab is a placeholder until its phase in docs/PLAN.md is implemented.
const PANELS: Record<TabId, { title: string; items: string[] }> = {
  docs: {
    title: "Claude Code 문서",
    items: [
      "프로젝트 설정 지도 (전역 · 프로젝트 · 로컬)",
      "skill / agent 라이브러리 적용 · 제외",
      "기술 스택 감지 후 CLAUDE.md 초안 생성",
    ],
  },
  work: {
    title: "작업 관리",
    items: ["브랜치 · worktree · 변경 상태", "브랜치 생성 · 전환 · 커밋", "GitHub PR · 이슈"],
  },
  memo: {
    title: "메모 / TODO",
    items: ["브랜치별 메모와 TODO", "여러 기기 간 동기화"],
  },
};

export default function SidebarApp() {
  const activeTab = useUiStore((s) => s.activeTab);
  const panel = PANELS[activeTab];

  useEffect(() => {
    const unsubscribe = subscribeDockStatus();
    return () => {
      void unsubscribe.then((stop) => stop());
    };
  }, []);

  // The sidebar stays loaded while the main window is used; catch up with what
  // changed there whenever it becomes the visible window again.
  useEffect(() => {
    restoreProject();
    if (!isTauri()) return;
    const unlisten = listen<ViewMode>("view-mode", (event) => {
      if (event.payload !== "sidebar") return;
      const before = useProjectStore.getState().current;
      restoreProject();
      if (useProjectStore.getState().current === before) void useProjectStore.getState().refresh();
      void useLibraryStore.getState().load();
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  return (
    <div className="flex h-full flex-col text-[13px]">
      <TitleBar />
      <TabNav />
      <main className="flex-1 overflow-y-auto p-4">
        {activeTab === "docs" ? (
          <DocsTab />
        ) : activeTab === "memo" ? (
          <MemoPanel compact />
        ) : (
          <Placeholder panel={panel} />
        )}
      </main>
      <StatusBar />
    </div>
  );
}

function Placeholder({ panel }: { panel: { title: string; items: string[] } }) {
  return (
    <>
      <h2 className="mb-3 text-sm font-semibold">{panel.title}</h2>
      <ul className="space-y-2">
        {panel.items.map((item) => (
          <li
            key={item}
            className="rounded-md border border-border bg-surface-raised px-3 py-2 text-text-muted"
          >
            {item}
          </li>
        ))}
      </ul>
      <p className="mt-4 text-xs text-text-muted">준비 중</p>
    </>
  );
}
