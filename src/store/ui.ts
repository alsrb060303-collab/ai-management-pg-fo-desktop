import { create } from "zustand";

export type TabId = "docs" | "work" | "memo";

interface UiState {
  activeTab: TabId;
  pinned: boolean;
  setActiveTab: (tab: TabId) => void;
  setPinned: (pinned: boolean) => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeTab: "docs",
  pinned: false,
  setActiveTab: (activeTab) => set({ activeTab }),
  setPinned: (pinned) => set({ pinned }),
}));
