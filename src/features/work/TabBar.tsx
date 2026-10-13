export interface TabItem<K extends string> {
  key: K;
  label: string;
  count?: number;
  /** Highlights the count (something waiting there). */
  dot?: boolean;
}

/** The work panel's tabs, with counts. Wraps in the narrow sidebar. */
export function TabBar<K extends string>({
  tabs,
  tab,
  onTab,
  compact,
}: {
  tabs: TabItem<K>[];
  tab: K;
  onTab: (tab: K) => void;
  compact: boolean;
}) {
  return (
    <div
      role="tablist"
      className={`flex gap-0.5 rounded-lg bg-sunken p-0.5 ${compact ? "flex-wrap" : ""}`}
    >
      {tabs.map((x) => (
        <button
          key={x.key}
          type="button"
          role="tab"
          aria-selected={tab === x.key}
          onClick={() => onTab(x.key)}
          className={`flex shrink-0 flex-1 items-center justify-center gap-1 whitespace-nowrap rounded-md px-2 py-1 ${
            compact ? "text-[11px]" : "text-xs"
          } ${
            tab === x.key
              ? "bg-surface-raised font-semibold text-text shadow-card"
              : "text-text-muted hover:text-text"
          }`}
        >
          {x.label}
          {x.count !== undefined && (
            <span
              className={`rounded-full px-1 text-[10px] tabular-nums ${
                x.dot ? "bg-accent-soft text-accent" : "text-text-subtle"
              }`}
            >
              {x.count}
            </span>
          )}
        </button>
      ))}
    </div>
  );
}
