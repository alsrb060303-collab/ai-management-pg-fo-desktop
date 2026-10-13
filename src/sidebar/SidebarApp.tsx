import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { CommandsPanel } from "../features/commands/CommandsPanel";
import { DocsTab } from "../features/docs/DocsTab";
import { MemoPanel } from "../features/memo/MemoPanel";
import { SyncNotice } from "../features/sync/SyncNotice";
import { useGitHubSync } from "../features/work/GitHubSection";
import { WorkPanel } from "../features/work/WorkPanel";
import { subscribeDockStatus } from "../store/dock";
import { useLibraryStore } from "../store/library";
import { restoreProject, useProjectStore } from "../store/project";
import { useSyncStore } from "../store/sync";
import { useUiStore } from "../store/ui";
import type { ViewMode } from "../store/view";
import { StatusBar } from "./StatusBar";
import { TabNav } from "./TabNav";
import { TitleBar } from "./TitleBar";

export default function SidebarApp() {
  // PR and CI state for the open project, and notifications, whatever page is shown.
  useGitHubSync();
  const activeTab = useUiStore((s) => s.activeTab);

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
    void useSyncStore.getState().init();
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
    <div className="flex h-full flex-col text-fs-13">
      <TitleBar />
      <TabNav />
      <main className="flex-1 overflow-y-auto p-4">
        {activeTab === "docs" ? (
          <DocsTab />
        ) : activeTab === "memo" ? (
          <MemoPanel compact />
        ) : activeTab === "commands" ? (
          <CommandsPanel compact />
        ) : (
          <WorkPanel compact />
        )}
      </main>
      <StatusBar />
      <SyncNotice />
    </div>
  );
}
