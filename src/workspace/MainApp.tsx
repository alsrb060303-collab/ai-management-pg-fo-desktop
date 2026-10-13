import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { lazy, Suspense, useEffect } from "react";
import { AccountDialog } from "../features/account/AccountDialog";
import { SyncNotice } from "../features/sync/SyncNotice";
import { UpdateNotice } from "../features/update/UpdateNotice";
import { useGitHubSync } from "../features/work/GitHubSection";
import { useAccountStore } from "../store/account";
import { useComposerStore } from "../store/composer";
import { subscribeDockStatus } from "../store/dock";
import { restoreProject } from "../store/project";
import { useSyncStore } from "../store/sync";
import { startUpdateChecks } from "../store/updater";
import { type Page, useWorkspaceStore } from "../store/workspace";
import { saveBeforeLeaving } from "./leave";
import { NavSidebar } from "./NavSidebar";
import { CommandsPage } from "./pages/CommandsPage";
import { ConfigPage } from "./pages/ConfigPage";
import { HomePage } from "./pages/HomePage";
import { LibraryPage } from "./pages/LibraryPage";
import { McpPage } from "./pages/McpPage";
import { MemoPage } from "./pages/MemoPage";
import { ProjectDocsPage } from "./pages/ProjectDocsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { WorkPage } from "./pages/WorkPage";

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
  files: () => <ProjectDocsPage />,
  mcp: () => <McpPage />,
  commands: () => <CommandsPage />,
  work: () => <WorkPage />,
  memo: () => <MemoPage />,
  settings: () => <SettingsPage />,
};

export default function MainApp() {
  // PR and CI state for the open project, and notifications, whatever page is shown.
  useGitHubSync();
  const page = useWorkspaceStore((s) => s.page);

  // Recent documents in the navigation, and the project selected last.
  useEffect(() => {
    void useComposerStore.getState().loadDocs();
    restoreProject();
    void useSyncStore.getState().init();
    void useAccountStore.getState().init();
    startUpdateChecks();
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
      <SyncNotice />
      <UpdateNotice />
      <AccountDialog />
    </div>
  );
}
