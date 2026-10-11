import { useComposerStore } from "../store/composer";
import { useMemoStore } from "../store/memo";
import { switchView } from "../store/view";

/**
 * Commits a focused draft and saves pending edits. The main window is
 * destroyed when it closes or switches to the sidebar, so this must run first.
 */
export async function saveBeforeLeaving() {
  (document.activeElement as HTMLElement | null)?.blur();
  await Promise.all([useComposerStore.getState().flushSave(), useMemoStore.getState().flushNote()]);
}

export async function switchToSidebar() {
  await saveBeforeLeaving();
  await switchView("sidebar");
}
