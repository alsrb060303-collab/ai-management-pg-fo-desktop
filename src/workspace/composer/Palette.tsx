import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useReactFlow } from "@xyflow/react";
import {
  ChevronDown,
  ExternalLink,
  Flag,
  Loader2,
  Plus,
  RefreshCw,
  Search,
  StickyNote,
} from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ResizeHandle, usePanelWidth } from "../../components/Resizable";
import { type MessageKey, useT } from "../../i18n";
import {
  type Block,
  type BlockKind,
  originUrl,
  type Target,
  targetAvailable,
  useComposerStore,
  useComposerTool,
} from "../../store/composer";
import { TOOLS, type ToolId } from "../../tools";
import { hooksFile, instructionsLabel, KIND_ICON, KIND_LABEL, kindColor, TARGETS } from "./kinds";
import { isLarge, LargeBlockDialog, largeWarningOff } from "./LargeBlockDialog";
import { dropIndex, PLAN_ENTRIES, type PlanEntry, setPlanDrop } from "./plans";
import { RatingBadges } from "./ratings";

type Filter = BlockKind | "all" | "plans";

const FILTERS: { value: Filter; label: MessageKey }[] = [
  { value: "all", label: "composer.palette.all" },
  { value: "plans", label: "composer.target.plans" },
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
  const addToCanvas = useComposerStore((s) => s.addBlock);
  /** A large block waiting for the user's OK before it's added. */
  const [pending, setPending] = useState<{ block: Block; at: { x: number; y: number } } | null>(
    null,
  );
  const addBlock = (block: Block, at: { x: number; y: number }) => {
    if (isLarge(block) && !largeWarningOff()) setPending({ block, at });
    else addToCanvas(block.id, at);
  };
  const addOutput = useComposerStore((s) => s.addOutput);
  const addMemo = useComposerStore((s) => s.addMemo);
  const tool = useComposerTool();
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
  const [filter, setFilter] = useState<Filter>("all");
  const [ghost, setGhost] = useState<{ title: string; x: number; y: number } | null>(null);
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
      setGhost({ title: block.title, x: ev.clientX, y: ev.clientY });
    };
    const row = e.currentTarget;
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      setGhost(null);
      setPlanDrop(null);
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
      if (position) addBlock(block, { x: position.x - 120, y: position.y - 30 });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    // A cancelled press (touch gesture, window switch) never gets its pointerup.
    window.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
  };

  const addOutputNode = (target: Target) => addOutput(target, place.nearCenter());

  /**
   * Puts a plan entry where it was dropped: a main goal on the canvas, the
   * rest into the plan node under the pointer (between its rows), else into
   * the selected or last plan, else into a new one.
   */
  const placePlan = (entry: PlanEntry, x: number, y: number) => {
    const store = useComposerStore.getState();
    const at = place.atPoint(x, y);
    if (entry === "main") {
      store.addPlan(at ? { x: at.x - 200, y: at.y - 40 } : place.nearCenter());
      return;
    }
    const kind = entry;
    const over = at ? document.elementFromPoint(x, y)?.closest("[data-plan-node]") : null;
    const overId = over?.getAttribute("data-plan-node");
    if (over && overId) {
      store.addPlanItem(overId, { kind }, dropIndex(over, y));
      return;
    }
    const plans = (store.doc?.nodes ?? []).filter((n) => n.type === "plan");
    const target = plans.find((n) => n.selected) ?? plans[plans.length - 1];
    const id = target?.id ?? store.addPlan(at ?? place.nearCenter());
    store.addPlanItem(id, { kind });
  };

  const startPlanDrag = (entry: PlanEntry, title: string, e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    (document.activeElement as HTMLElement | null)?.blur();
    const start = { x: e.clientX, y: e.clientY };
    let dragging = false;
    const move = (ev: PointerEvent) => {
      if (!dragging && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 5) return;
      dragging = true;
      setGhost({ title, x: ev.clientX, y: ev.clientY });
      // Show where a goal or step would go in the plan under the pointer.
      const over =
        entry === "main"
          ? null
          : document.elementFromPoint(ev.clientX, ev.clientY)?.closest("[data-plan-node]");
      const node = over?.getAttribute("data-plan-node");
      setPlanDrop(over && node ? { node, gap: dropIndex(over, ev.clientY) } : null);
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      setGhost(null);
      setPlanDrop(null);
    };
    const up = (ev: PointerEvent) => {
      stop();
      // A click adds it too (to the selected or last plan).
      if (!dragging) placePlan(entry, -1, -1);
      else if (place.atPoint(ev.clientX, ev.clientY)) placePlan(entry, ev.clientX, ev.clientY);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
  };

  const q = query.trim().toLowerCase();
  const planEntries =
    filter === "all" || filter === "plans"
      ? PLAN_ENTRIES.filter((p) => !q || t(p.label).toLowerCase().includes(q))
      : [];
  const panel = usePanelWidth("composer-palette", 256, 200, 480);

  return (
    <aside
      className="relative flex shrink-0 flex-col border-r border-border bg-sidebar"
      style={{ width: panel.width }}
    >
      <ResizeHandle side="right" {...panel.handle} />
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
        <div className="flex items-center gap-1.5 px-1 text-fs-11 text-text-subtle">
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
        {planEntries.length > 0 && (
          <section>
            <h3
              className="mb-1.5 flex items-center gap-1.5 px-1 text-xs font-semibold"
              style={{ color: kindColor("plan") }}
            >
              <Flag size={13} />
              {t("composer.target.plans")}
            </h3>
            <ul className="space-y-1">
              {planEntries.map((p) => (
                <li key={p.entry}>
                  <div
                    onPointerDown={(e) => startPlanDrag(p.entry, t(p.label), e)}
                    title={t("composer.plan.paletteHint")}
                    className={`flex cursor-grab touch-none items-start gap-2 rounded-lg border border-transparent py-1.5 pr-2 hover:border-border hover:bg-surface-raised active:cursor-grabbing ${
                      p.entry === "main" ? "pl-2" : p.entry === "sub" ? "pl-4" : "pl-6"
                    }`}
                  >
                    <span
                      className={`mt-1.5 shrink-0 ${p.entry === "step" ? "size-2 rotate-45" : "size-2 rounded-full"}`}
                      style={{ background: kindColor("plan") }}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="text-fs-13 font-medium">{t(p.label)}</div>
                      <div className="text-xs leading-snug text-text-muted">{t(p.hint)}</div>
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          </section>
        )}
        {groups.length === 0 && planEntries.length === 0 && (
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
                        <div className="flex items-center gap-1.5 text-fs-13 font-medium">
                          <span className="truncate">{block.title}</span>
                          {block.source === "library" && (
                            <span className="shrink-0 rounded bg-sunken px-1 text-fs-10 text-text-subtle">
                              {t("composer.palette.myLibrary")}
                            </span>
                          )}
                          {block.origin && (
                            <span
                              className="shrink-0 rounded bg-sunken px-1 text-fs-10 text-text-subtle"
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
                        {block.rating && <RatingBadges rating={block.rating} />}
                      </div>
                      <button
                        type="button"
                        aria-label={t("composer.palette.add", { title: block.title })}
                        title={t("composer.palette.addToCanvas")}
                        onPointerDown={(e) => e.stopPropagation()}
                        onClick={() => addBlock(block, place.nearCenter())}
                        className="mt-0.5 rounded-md p-0.5 text-text-subtle opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-text focus-visible:opacity-100"
                      >
                        <Plus size={15} />
                      </button>
                    </div>
                    {opened === block.id && (
                      <BlockDetail
                        block={block}
                        tool={tool}
                        onAdd={() => addBlock(block, place.nearCenter())}
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
        <h3
          className="mb-1.5 flex items-baseline gap-1.5 px-1 text-xs font-semibold text-text-muted"
          title={t("composer.palette.toolHint", { tool: TOOLS[tool].name })}
        >
          {t("composer.palette.addOutput")}
          <span className="ml-auto truncate font-normal text-text-subtle">
            {t("composer.palette.forTool", { tool: TOOLS[tool].name })}
          </span>
        </h3>
        <div className="grid grid-cols-2 gap-1.5">
          {TARGETS.filter((m) => targetAvailable(m.target, tool)).map(
            ({ target, label, icon: Icon }) => (
              <button
                key={target}
                type="button"
                onClick={() => addOutputNode(target)}
                className="flex items-center gap-1.5 rounded-lg border border-border bg-surface-raised px-2 py-1.5 text-xs hover:border-border-strong"
              >
                <Icon size={13} className="text-text-muted" />
                {label ? t(label) : instructionsLabel(tool)}
              </button>
            ),
          )}
          <button
            type="button"
            onClick={() => addMemo(place.nearCenter())}
            title={t("composer.palette.memoHint")}
            className="flex items-center gap-1.5 rounded-lg border border-dashed border-memo-border bg-memo px-2 py-1.5 text-xs hover:border-border-strong"
          >
            <StickyNote size={13} className="text-warning" />
            {t("composer.palette.memo")}
          </button>
        </div>
      </div>

      {ghost &&
        createPortal(
          <div
            className="pointer-events-none fixed z-50 w-56 -translate-x-1/2 -translate-y-1/2 rounded-xl border border-accent bg-surface-raised px-3 py-2 text-sm font-medium shadow-card"
            style={{ left: ghost.x, top: ghost.y }}
          >
            {ghost.title}
          </div>,
          document.body,
        )}
      {pending &&
        createPortal(
          <LargeBlockDialog
            block={pending.block}
            onAdd={() => {
              addToCanvas(pending.block.id, pending.at);
              setPending(null);
            }}
            onCancel={() => setPending(null)}
          />,
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
function BlockDetail({ block, tool, onAdd }: { block: Block; tool: ToolId; onAdd: () => void }) {
  const t = useT();
  const [showText, setShowText] = useState(false);
  const info = TOOLS[tool];
  const dirs: Record<BlockKind, string> = {
    instruction: info.instructions.team,
    rule: info.rulesDir ?? info.instructions.team,
    skill: info.skillsDir,
    agent: info.agentsDir,
    hook: hooksFile(tool, false),
  };
  const params = { tool: info.name, file: info.instructions.team, dir: dirs[block.kind] };
  const where =
    block.kind === "rule" && !info.rulesDir
      ? t("composer.detail.where.ruleNoRules", params)
      : block.kind === "rule" && block.paths.length
        ? t("composer.detail.where.ruleFor", { ...params, paths: block.paths.join(", ") })
        : t(KIND_WHERE[block.kind], params);
  return (
    <div className="mx-1 mt-1 mb-2 space-y-2 rounded-lg border border-border bg-surface-raised p-2.5 text-xs">
      {block.detail && <p className="leading-relaxed text-text">{block.detail}</p>}
      <p className="text-text-muted">{where}</p>
      {block.extraFiles > 0 && (
        <p className="text-warning">{t("composer.detail.extraFiles", { n: block.extraFiles })}</p>
      )}
      {block.companions.length > 0 && (
        <details className="text-text-muted">
          <summary className="cursor-pointer">
            {t("composer.detail.companions", { n: block.companions.length })}
          </summary>
          <ul className="mt-1 space-y-0.5 pl-3 font-mono text-fs-11">
            {block.companions.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        </details>
      )}
      {block.origin && (
        <button
          type="button"
          onClick={() => block.origin && openLink(originUrl(block.origin))}
          className="flex items-center gap-1 text-fs-11 text-text-subtle hover:text-text"
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
        <pre className="selectable max-h-56 overflow-auto rounded-md bg-sunken p-2 font-mono text-fs-11 leading-relaxed whitespace-pre-wrap">
          {block.body}
        </pre>
      )}
    </div>
  );
}
