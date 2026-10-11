import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { lazy, Suspense, useEffect } from "react";
import { useComposerStore } from "../store/composer";
import { subscribeDockStatus } from "../store/dock";
import { restoreProject } from "../store/project";
import { type Page, useWorkspaceStore } from "../store/workspace";
import { saveBeforeLeaving } from "./leave";
import { NavSidebar } from "./NavSidebar";
import { ComingSoonPage } from "./pages/ComingSoonPage";
import { ConfigPage } from "./pages/ConfigPage";
import { HomePage } from "./pages/HomePage";
import { LibraryPage } from "./pages/LibraryPage";
import { MemoPage } from "./pages/MemoPage";
import { SettingsPage } from "./pages/SettingsPage";

// The graph editor is the heaviest page; load it on first use.
const ComposerPage = lazy(() => import("./composer/ComposerPage"));

const PAGES: Record<Page, () => React.ReactNode> = {
  home: () => <HomePage />,
  composer: () => (
    <Suspense fallback={null}>
      <ComposerPage />
    </Suspense>
  ),
  library: () => <LibraryPage />,
  config: () => <ConfigPage />,
  work: () => (
    <ComingSoonPage
      title="작업"
      description="브랜치 · worktree · 변경 상태와 GitHub PR · 이슈를 여기서 관리하게 됩니다."
    />
  ),
  memo: () => <MemoPage />,
  settings: () => <SettingsPage />,
};

export default function MainApp() {
  const page = useWorkspaceStore((s) => s.page);

  // Recent documents in the navigation, and the project selected last.
  useEffect(() => {
    void useComposerStore.getState().loadDocs();
    restoreProject();
  }, []);

  // The window is destroyed on close (also when switching to the sidebar from
  // the tray or shortcut); save the open document first.
  useEffect(() => {
    if (!isTauri()) return;
    const unlisten = getCurrentWindow().onCloseRequested(() => saveBeforeLeaving());
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

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
      <main
        className={`min-w-0 flex-1 ${page === "composer" ? "overflow-hidden" : "overflow-y-auto"}`}
      >
        {PAGES[page]()}
      </main>
    </div>
  );
}
