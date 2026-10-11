import "pretendard/dist/web/variable/pretendardvariable-dynamic-subset.css";
import "@fontsource-variable/source-serif-4";
import "@fontsource-variable/jetbrains-mono";
import "./index.css";

import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import { initTheme } from "./store/theme";

// One bundle, two windows: the window label picks the UI, and each UI is
// loaded on its own so the sidebar never pulls in the workspace code.
// In a plain browser (vite dev), `?view=sidebar` previews the sidebar.
const label = isTauri()
  ? getCurrentWebviewWindow().label
  : (new URLSearchParams(window.location.search).get("view") ?? "main");

const Root =
  label === "sidebar"
    ? lazy(() => import("./sidebar/SidebarApp"))
    : lazy(() => import("./workspace/MainApp"));

initTheme();

// Dropping a file or folder on the window would make the webview navigate to
// it and unload the app. Nothing here accepts file drops yet.
for (const type of ["dragover", "drop"] as const) {
  document.addEventListener(type, (event) => {
    if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
  });
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Suspense fallback={null}>
      <Root />
    </Suspense>
  </React.StrictMode>,
);
