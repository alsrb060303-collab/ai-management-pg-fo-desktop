import { useState } from "react";
import { ConfigMapPanel } from "./ConfigMapPanel";
import { LibraryPanel } from "./LibraryPanel";

type View = "map" | "library";

const VIEWS: { id: View; label: string }[] = [
  { id: "map", label: "설정 지도" },
  { id: "library", label: "라이브러리" },
];

export function DocsTab() {
  const [view, setView] = useState<View>("map");
  return (
    <div className="space-y-3">
      <div className="flex rounded-md border border-border p-0.5 text-xs">
        {VIEWS.map((v) => (
          <button
            key={v.id}
            type="button"
            aria-pressed={view === v.id}
            onClick={() => setView(v.id)}
            className={`flex-1 rounded py-1 ${view === v.id ? "bg-surface-raised font-semibold" : "text-text-muted hover:text-text"}`}
          >
            {v.label}
          </button>
        ))}
      </div>
      {view === "map" ? <ConfigMapPanel /> : <LibraryPanel />}
    </div>
  );
}
