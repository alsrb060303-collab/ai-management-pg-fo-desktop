import { create } from "zustand";

export type WorkTab = "changes" | "history" | "branches" | "worktrees" | "github";

const TAB_KEY = "work.tab";
const TABS: WorkTab[] = ["changes", "history", "branches", "worktrees", "github"];

function loadTab(): WorkTab {
  try {
    const tab = localStorage.getItem(TAB_KEY) as WorkTab | null;
    return tab && TABS.includes(tab) ? tab : "changes";
  } catch {
    return "changes";
  }
}

interface WorkUi {
  tab: WorkTab;
  /** The pull request form is open on the GitHub tab. */
  creatingPull: boolean;
  /** Bumped to move focus to the commit message. */
  commitFocus: number;
  setTab: (tab: WorkTab) => void;
  setCreatingPull: (open: boolean) => void;
  /** Shows the changes tab and focuses the commit message. */
  focusCommit: () => void;
}

/** What the work panel shows; the flow card's buttons steer it. */
export const useWorkUi = create<WorkUi>((set) => ({
  tab: loadTab(),
  creatingPull: false,
  commitFocus: 0,
  setTab: (tab) => {
    try {
      localStorage.setItem(TAB_KEY, tab);
    } catch {
      // Only a convenience.
    }
    set({ tab });
  },
  setCreatingPull: (creatingPull) => set({ creatingPull }),
  focusCommit: () => {
    useWorkUi.getState().setTab("changes");
    set((s) => ({ commitFocus: s.commitFocus + 1 }));
  },
}));
