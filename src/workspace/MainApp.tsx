import { useEffect } from "react";
import { subscribeDockStatus } from "../store/dock";
import { type Page, useWorkspaceStore } from "../store/workspace";
import { NavSidebar } from "./NavSidebar";
import { ComingSoonPage } from "./pages/ComingSoonPage";
import { ComposerPage } from "./pages/ComposerPage";
import { ConfigPage } from "./pages/ConfigPage";
import { HomePage } from "./pages/HomePage";
import { LibraryPage } from "./pages/LibraryPage";
import { SettingsPage } from "./pages/SettingsPage";

const PAGES: Record<Page, () => React.ReactNode> = {
  home: () => <HomePage />,
  composer: () => <ComposerPage />,
  library: () => <LibraryPage />,
  config: () => <ConfigPage />,
  work: () => (
    <ComingSoonPage
      title="작업"
      description="브랜치 · worktree · 변경 상태와 GitHub PR · 이슈를 여기서 관리하게 됩니다."
    />
  ),
  memo: () => (
    <ComingSoonPage
      title="메모"
      description="브랜치별 메모와 TODO를 남기고 여러 기기에서 이어서 볼 수 있게 됩니다."
    />
  ),
  settings: () => <SettingsPage />,
};

export default function MainApp() {
  const page = useWorkspaceStore((s) => s.page);

  // Settings show and change the sidebar's docking state.
  useEffect(() => {
    const unsubscribe = subscribeDockStatus();
    return () => {
      void unsubscribe.then((stop) => stop());
    };
  }, []);

  return (
    <div className="flex h-full">
      <NavSidebar />
      <main className="min-w-0 flex-1 overflow-y-auto">{PAGES[page]()}</main>
    </div>
  );
}
