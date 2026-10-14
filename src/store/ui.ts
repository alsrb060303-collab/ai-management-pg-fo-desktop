import { create } from "zustand";

export type TabId = "docs" | "work" | "memo" | "commands" | "trees";

interface UiState {
  activeTab: TabId;
  setActiveTab: (tab: TabId) => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeTab: "docs",
  setActiveTab: (activeTab) => set({ activeTab }),
}));
