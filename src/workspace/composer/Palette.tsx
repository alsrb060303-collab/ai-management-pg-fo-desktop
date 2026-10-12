import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useReactFlow } from "@xyflow/react";
import { ChevronDown, ExternalLink, Loader2, Plus, RefreshCw, Search } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { type MessageKey, useT } from "../../i18n";
import {
  type Block,
  type BlockKind,
  originUrl,
  type Target,
  useComposerStore,
} from "../../store/composer";
import { KIND_ICON, KIND_LABEL, kindColor, TARGETS } from "./kinds";

const FILTERS: { value: BlockKind | "all"; label: MessageKey }[] = [
  { value: "all", label: "composer.palette.all" },
  { value: "instruction", label: "composer.kind.instruction" },
  { value: "rule", label: "composer.kind.rule" },
  { value: "skill", label: "composer.kind.skill" },
  { value: "agent", label: "composer.palette.agent" },
  { value: "hook", label: "composer.palette.hook" },
];

const KIND_ORDER: BlockKind[] = ["instruction", "rule", "skill", "agent", "hook"];

/** Adds nodes where the user drops them, or left of center on click (outputs sit in the middle). */
function usePlacement() {
  const flow = useReactFlow();
  const added = useRef(0);
  const canvasRect = () => document.querySelector(".react-flow")?.getBoundingClientRect() ?? null;

  const atPoint = (x: number, y: number) => {
    const rect = canvasRect();
    if (!rect || x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) return null;
    return flow.screenToFlowPosition({ x, y });
  };

  const nearCenter = () => {
    const rect = canvasRect();
    const offset = (added.current++ % 6) * 24;
    const center = rect
      ? { x: rect.left + rect.width * 0.08 + offset, y: rect.top + rect.height * 0.2 + offset }
      : { x: 200 + offset, y: 200 + offset };
    return flow.screenToFlowPosition(center);
  };
  return { atPoint, nearCenter };
}

export function Palette() {
  const catalog = useComposerStore((s) => s.catalog);
  const addBlock = useComposerStore((s) => s.addBlock);
  const addOutput = useComposerStore((s) => s.addOutput);
  const refreshExternal = useComposerStore((s) => s.refreshExternal);
  const t = useT();
  const [refreshing, setRefreshing] = useState(false);
  const [refreshNote, setRefreshNote] = useState<string | null>(null);
  const refresh = async () => {
    setRefreshing(true);
    setRefreshNote(null);
    try {
      setRefreshNote(await refreshExternal());
    } catch (e) {
      setRefreshNote(String(e));
    } finally {
      setRefreshing(false);
    }
  };
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<BlockKind | "all">("all");
  const [ghost, setGhost] = useState<{ block: Block; x: number; y: number } | null>(null);
  /** The block whose details are open. */
  const [opened, setOpened] = useState<string | null>(null);
  const place = usePlacement();

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const match = (b: Block) =>
      (filter === "all" || b.kind === filter) &&
      (!q || [b.title, b.summary, b.category, b.name].some((s) => s.toLowerCase().includes(q)));
    return KIND_ORDER.map((kind) => ({
      kind,
      blocks: catalog.filter((b) => b.kind === kind && match(b)),
    })).filter((g) => g.blocks.length > 0);
  }, [catalog, query, filter]);

  // Pointer-based drag (HTML5 drag and drop is unreliable in WebKitGTK).
  const startDrag = (block: Block, e: React.PointerEvent) => {
    if (e.button !== 0) return;
    // No text selection while dragging. Commit a focused draft (block editor,
    // document name) first, since preventDefault keeps it from blurring.
    e.preventDefault();
    (document.activeElement as HTMLElement | null)?.blur();
    const start = { x: e.clientX, y: e.clientY };
    let dragging = false;
    const move = (ev: PointerEvent) => {
      if (!dragging && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 5) return;
      dragging = true;
      setGhost({ block, x: ev.clientX, y: ev.clientY });
    };
    const row = e.currentTarget;
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      setGhost(null);
    };
    const up = (ev: PointerEvent) => {
      stop();
      // A click on the row without dragging opens (or closes) its details.
      if (!dragging) {
        if (ev.target instanceof Node && row.contains(ev.target)) {
          setOpened((id) => (id === block.id ? null : block.id));
        }
        return;
      }
      const position = place.atPoint(ev.clientX, ev.clientY);
      if (position) addBlock(block.id, { x: position.x - 120, y: position.y - 30 });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    // A cancelled press (touch gesture, window switch) never gets its pointerup.
    window.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
  };

  const addOutputNode = (target: Target) => addOutput(target, place.nearCenter());

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-sidebar">
      <div className="space-y-2 p-3">
        <label className="flex h-9 items-center gap-2 rounded-lg border border-border bg-surface-raised px-2.5 focus-within:border-border-strong">
          <Search size={15} className="shrink-0 text-text-subtle" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("composer.palette.search")}
            className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-text-subtle"
          />
        </label>
        <div className="flex flex-wrap items-center gap-1">
          {FILTERS.map((f) => (
            <button
              key={f.value}
              type="button"
              aria-pressed={filter === f.value}
              onClick={() => setFilter(f.value)}
              className={`rounded-full px-2 py-0.5 text-xs whitespace-nowrap transition-colors ${
                filter === f.value
                  ? "bg-primary text-primary-fg"
                  : "bg-sunken text-text-muted hover:text-text"
              }`}
            >
              {t(f.label)}
            </button>
          ))}
        </div>
        <div className="flex items-center gap-1.5 px-1 text-[11px] text-text-subtle">
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={refreshing}
            title={t("composer.palette.refreshHint")}
            className="flex items-center gap-1 rounded-md py-0.5 hover:text-text disabled:opacity-60"
          >
            {refreshing ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
            {t("composer.palette.refresh")}
          </button>
          {refreshNote && (
            <span className="truncate" title={refreshNote}>
              · {refreshNote}
            </span>
          )}
        </div>
      </div>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-3 pb-3">
        {groups.length === 0 && (
          <p className="px-1 text-xs text-text-subtle">
            {catalog.length === 0 ? t("composer.palette.loading") : t("composer.palette.noMatch")}
          </p>
        )}
        {groups.map(({ kind, blocks }) => {
          const Icon = KIND_ICON[kind];
          return (
            <section key={kind}>
              <h3
                className="mb-1.5 flex items-center gap-1.5 px-1 text-xs font-semibold"
                style={{ color: kindColor(kind) }}
              >
                <Icon size={13} />
                {t(KIND_LABEL[kind])}
                <span className="font-normal text-text-subtle">{blocks.length}</span>
              </h3>
              <ul className="space-y-1">
                {blocks.map((block) => (
                  <li key={block.id}>
                    <div
                      onPointerDown={(e) => startDrag(block, e)}
                      title={t("composer.palette.dragHint")}
                      className="group flex cursor-grab touch-none items-start gap-2 rounded-lg border border-transparent px-2 py-1.5 hover:border-border hover:bg-surface-raised active:cursor-grabbing"
                    >
                      <span
                        className="mt-1.5 size-2 shrink-0 rounded-full"
                        style={{ background: kindColor(block.kind) }}
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-1.5 text-[13px] font-medium">
                          <span className="truncate">{block.title}</span>
                          {block.source === "library" && (
                            <span className="shrink-0 rounded bg-sunken px-1 text-[10px] text-text-subtle">
                              {t("composer.palette.myLibrary")}
                            </span>
                          )}
                          {block.origin && (
                            <span
                              className="shrink-0 rounded bg-sunken px-1 text-[10px] text-text-subtle"
                              title={`${block.origin.repo} · ${block.origin.license}`}
                            >
                              {block.origin.repo.split("/")[0]}
                            </span>
                          )}
                        </div>
                        <div
                          className={`text-xs leading-snug text-text-muted ${opened === block.id ? "" : "line-clamp-2"}`}
                        >
                          {block.summary}
                        </div>
                      </div>
                      <button
                        type="button"
                        aria-label={t("composer.palette.add", { title: block.title })}
                        title={t("composer.palette.addToCanvas")}
                        onPointerDown={(e) => e.stopPropagation()}
                        onClick={() => addBlock(block.id, place.nearCenter())}
                        className="mt-0.5 rounded-md p-0.5 text-text-subtle opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-text focus-visible:opacity-100"
                      >
                        <Plus size={15} />
                      </button>
                    </div>
                    {opened === block.id && (
                      <BlockDetail
                        block={block}
                        onAdd={() => addBlock(block.id, place.nearCenter())}
                      />
                    )}
                  </li>
                ))}
              </ul>
            </section>
          );
        })}
      </div>

      <div className="border-t border-border p-3">
        <h3 className="mb-1.5 px-1 text-xs font-semibold text-text-muted">
          {t("composer.palette.addOutput")}
        </h3>
        <div className="grid grid-cols-2 gap-1.5">
          {TARGETS.map(({ target, label, icon: Icon }) => (
            <button
              key={target}
              type="button"
              onClick={() => addOutputNode(target)}
              className="flex items-center gap-1.5 rounded-lg border border-border bg-surface-raised px-2 py-1.5 text-xs hover:border-border-strong"
            >
              <Icon size={13} className="text-text-muted" />
              {t(label)}
            </button>
          ))}
        </div>
      </div>

      {ghost &&
        createPortal(
          <div
            className="pointer-events-none fixed z-50 w-56 -translate-x-1/2 -translate-y-1/2 rounded-xl border border-accent bg-surface-raised px-3 py-2 text-sm font-medium shadow-card"
            style={{ left: ghost.x, top: ghost.y }}
          >
            {ghost.block.title}
          </div>,
          document.body,
        )}
    </aside>
  );
}

/** Where each kind of block ends up, explained once per kind. */
const KIND_WHERE: Record<BlockKind, MessageKey> = {
  instruction: "composer.detail.where.instruction",
  rule: "composer.detail.where.rule",
  skill: "composer.detail.where.skill",
  agent: "composer.detail.where.agent",
  hook: "composer.detail.where.hook",
};

const openLink = (url: string) => {
  if (isTauri()) void openUrl(url);
  else window.open(url, "_blank");
};

/** A block's details: what it does, where it goes, where it's from, and its text. */
function BlockDetail({ block, onAdd }: { block: Block; onAdd: () => void }) {
  const t = useT();
  const [showText, setShowText] = useState(false);
  const where =
    block.kind === "rule" && block.paths.length
      ? t("composer.detail.where.ruleFor", { paths: block.paths.join(", ") })
      : t(KIND_WHERE[block.kind]);
  return (
    <div className="mx-1 mt-1 mb-2 space-y-2 rounded-lg border border-border bg-surface-raised p-2.5 text-xs">
      {block.detail && <p className="leading-relaxed text-text">{block.detail}</p>}
      <p className="text-text-muted">{where}</p>
      {block.extraFiles > 0 && (
        <p className="text-warning">{t("composer.detail.extraFiles", { n: block.extraFiles })}</p>
      )}
      {block.origin && (
        <button
          type="button"
          onClick={() => block.origin && openLink(originUrl(block.origin))}
          className="flex items-center gap-1 text-[11px] text-text-subtle hover:text-text"
          title={block.origin.copyright}
        >
          {block.origin.repo} · {block.origin.license}
          <ExternalLink size={10} />
        </button>
      )}
      <div className="flex items-center gap-1.5">
        <button
          type="button"
          onClick={() => setShowText(!showText)}
          className="flex items-center gap-0.5 text-text-muted hover:text-text"
        >
          <ChevronDown size={12} className={showText ? "rotate-180" : ""} />
          {t("composer.detail.showText")}
        </button>
        <button
          type="button"
          onClick={onAdd}
          className="ml-auto flex items-center gap-1 rounded-md bg-primary px-2 py-0.5 font-medium text-primary-fg hover:bg-primary-hover"
        >
          <Plus size={12} />
          {t("composer.detail.add")}
        </button>
      </div>
      {showText && (
        <pre className="selectable max-h-56 overflow-auto rounded-md bg-sunken p-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap">
          {block.body}
        </pre>
      )}
    </div>
  );
}
