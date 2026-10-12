import { type MessageKey, useT } from "../i18n";
import { type TabId, useUiStore } from "../store/ui";

const TABS: { id: TabId; label: MessageKey }[] = [
  { id: "docs", label: "shell.tab.docs" },
  { id: "work", label: "shell.tab.work" },
  { id: "memo", label: "shell.tab.memo" },
  { id: "commands", label: "shell.tab.commands" },
];

export function TabNav() {
  const activeTab = useUiStore((s) => s.activeTab);
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const t = useT();

  return (
    <div role="tablist" className="flex shrink-0 border-b border-border">
      {TABS.map((tab) => {
        const selected = tab.id === activeTab;
        return (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={selected}
            onClick={() => setActiveTab(tab.id)}
            className={`flex-1 py-2 text-sm transition-colors ${
              selected
                ? "border-b-2 border-accent font-semibold text-text"
                : "border-b-2 border-transparent text-text-muted hover:text-text"
            }`}
          >
            {t(tab.label)}
          </button>
        );
      })}
    </div>
  );
}
