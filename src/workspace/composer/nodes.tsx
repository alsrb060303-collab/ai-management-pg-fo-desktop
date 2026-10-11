import { Handle, type NodeProps, Position, useReactFlow } from "@xyflow/react";
import { GripVertical, PencilLine, Trash2, X } from "lucide-react";
import { memo, useRef, useState } from "react";
import { useT } from "../../i18n";
import {
  ACCEPTS,
  type BlockNode,
  CLAUDE_MD_LINE_BUDGET,
  inputsOf,
  type OutputNode,
  useComposerStore,
} from "../../store/composer";
import { KIND_ICON, KIND_LABEL, kindColor, TARGETS, targetTitle } from "./kinds";

/**
 * Removes the node through React Flow, so undo, edge clean-up and the output
 * order go the same way as the Delete key.
 */
function DeleteNodeButton({ id, label }: { id: string; label: string }) {
  const { deleteElements } = useReactFlow();
  return (
    <button
      type="button"
      aria-label={label}
      title={`${label} (Delete)`}
      onClick={(e) => {
        e.stopPropagation();
        void deleteElements({ nodes: [{ id }] });
      }}
      className="nodrag rounded-md p-1 text-text-subtle opacity-0 transition-opacity group-hover:opacity-100 hover:bg-hover hover:text-danger focus-visible:opacity-100"
    >
      <Trash2 size={13} />
    </button>
  );
}

export const BlockNodeView = memo(function BlockNodeView({
  id,
  data,
  selected,
}: NodeProps<BlockNode>) {
  const block = useComposerStore((s) => s.blocks.get(data.blockId));
  const kind = block?.kind ?? "instruction";
  const Icon = KIND_ICON[kind];
  const color = kindColor(kind);
  const t = useT();

  return (
    <div
      className={`group w-60 rounded-xl border bg-surface-raised shadow-card transition-colors ${
        selected ? "border-accent ring-2 ring-accent-soft" : "border-border"
      }`}
    >
      <div className="h-1 rounded-t-xl" style={{ background: color }} />
      <div className="px-3 pt-2 pb-2.5">
        <div className="flex items-center gap-1.5 text-[11px] font-medium" style={{ color }}>
          <Icon size={12} />
          {t(KIND_LABEL[kind])}
          {block?.category && <span className="text-text-subtle">· {block.category}</span>}
          <span className="ml-auto flex items-center gap-1">
            {data.body !== undefined && (
              <span className="flex items-center gap-0.5 text-text-subtle">
                <PencilLine size={11} />
                {t("composer.node.edited")}
              </span>
            )}
            <span className={selected ? "[&>button]:opacity-100" : ""}>
              <DeleteNodeButton id={id} label={t("composer.node.deleteBlock")} />
            </span>
          </span>
        </div>
        <div className="mt-1 text-sm leading-snug font-medium">
          {block?.title ?? (
            <span className="text-danger">
              {t("composer.node.missingBlock", { id: data.blockId })}
            </span>
          )}
        </div>
        {block?.summary && (
          <div className="mt-0.5 line-clamp-2 text-xs leading-snug text-text-muted">
            {block.summary}
          </div>
        )}
      </div>
      <Handle
        type="source"
        position={Position.Right}
        style={{ background: color, borderColor: "var(--c-raised)" }}
      />
    </div>
  );
});

export const OutputNodeView = memo(function OutputNodeView({
  id,
  data,
  selected,
}: NodeProps<OutputNode>) {
  const doc = useComposerStore((s) => s.doc);
  const blocks = useComposerStore((s) => s.blocks);
  const file = useComposerStore((s) => s.compiled?.files.find((f) => f.node === id));
  const fileCount = useComposerStore((s) => s.compiled?.files.filter((f) => f.node === id).length);
  const setScope = useComposerStore((s) => s.setScope);
  const reorder = useComposerStore((s) => s.reorder);
  const { deleteElements } = useReactFlow();
  const t = useT();

  const personal = data.scope === "personal";
  const meta = TARGETS.find((m) => m.target === data.target) ?? TARGETS[0];
  const Icon = meta.icon;
  const inputs = doc ? inputsOf(doc, id) : [];
  const isClaudeMd = data.target === "claudeMd";
  const lines = isClaudeMd ? (file?.lines ?? 0) : 0;
  const over = lines > CLAUDE_MD_LINE_BUDGET;

  const titleOf = (nodeId: string) => {
    const node = doc?.nodes.find((n) => n.id === nodeId);
    if (node?.type !== "block") return { title: nodeId, kind: undefined };
    const block = blocks.get(node.data.blockId);
    return { title: block?.title ?? node.data.blockId, kind: block?.kind };
  };

  return (
    <div
      className={`group w-72 rounded-2xl border-2 bg-surface-raised shadow-card transition-colors ${
        selected ? "border-accent" : "border-border-strong"
      }`}
    >
      <Handle
        type="target"
        position={Position.Left}
        style={{ width: 14, height: 14, background: "var(--c-raised)" }}
      />
      <div className="flex items-center gap-2 border-b border-border px-3.5 py-2.5">
        <span className="flex size-7 items-center justify-center rounded-lg bg-sunken text-text-muted">
          <Icon size={15} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate font-mono text-[13px] font-semibold">
            {targetTitle(data.target, personal)}
          </div>
          <div className="text-[11px] text-text-subtle">
            {t("composer.node.accepts", {
              kinds: ACCEPTS[data.target].map((k) => t(KIND_LABEL[k])).join(" · "),
            })}
          </div>
        </div>
        <fieldset
          aria-label={t("composer.node.scope")}
          className="nodrag flex rounded-md bg-sunken p-0.5 text-[11px]"
        >
          {(["team", "personal"] as const).map((scope) => (
            <button
              key={scope}
              type="button"
              aria-pressed={data.scope === scope}
              title={
                scope === "team" ? t("composer.node.teamHint") : t("composer.node.personalHint")
              }
              onClick={() => setScope(id, scope)}
              className={`rounded px-1.5 py-0.5 font-medium ${
                data.scope === scope ? "bg-surface-raised text-text shadow-card" : "text-text-muted"
              }`}
            >
              {scope === "team" ? t("composer.node.team") : t("composer.personal")}
            </button>
          ))}
        </fieldset>
        <span className={selected ? "[&>button]:opacity-100" : ""}>
          <DeleteNodeButton id={id} label={t("composer.node.deleteOutput")} />
        </span>
      </div>

      {inputs.length === 0 ? (
        <p className="px-3.5 py-4 text-center text-xs text-text-subtle">
          {t("composer.node.empty")}
        </p>
      ) : (
        <OrderedInputs
          ids={inputs}
          titleOf={titleOf}
          reorderable={isClaudeMd}
          onReorder={(from, to) => reorder(id, from, to)}
          onDisconnect={(source) => {
            const edges = (doc?.edges ?? [])
              .filter((e) => e.source === source && e.target === id)
              .map((e) => ({ id: e.id }));
            void deleteElements({ edges });
          }}
        />
      )}

      <div className="flex items-center gap-2 border-t border-border px-3.5 py-2 text-[11px] text-text-muted">
        {isClaudeMd ? (
          <>
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-sunken">
              <div
                className={`h-full rounded-full ${over ? "bg-danger" : "bg-accent"}`}
                style={{ width: `${Math.min(100, (lines / CLAUDE_MD_LINE_BUDGET) * 100)}%` }}
              />
            </div>
            <span className={over ? "font-medium text-danger" : ""}>
              {t("composer.node.lineBudget", { n: lines, max: CLAUDE_MD_LINE_BUDGET })}
            </span>
          </>
        ) : (
          <span>{t("composer.node.fileCount", { n: fileCount ?? 0 })}</span>
        )}
      </div>
    </div>
  );
});

/** The output's connected blocks; CLAUDE.md order can be changed by dragging. */
function OrderedInputs({
  ids,
  titleOf,
  reorderable,
  onReorder,
  onDisconnect,
}: {
  ids: string[];
  titleOf: (id: string) => { title: string; kind?: keyof typeof KIND_LABEL };
  reorderable: boolean;
  onReorder: (from: number, to: number) => void;
  onDisconnect: (nodeId: string) => void;
}) {
  const listRef = useRef<HTMLOListElement>(null);
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  const t = useT();

  // Pointer events rather than HTML5 drag and drop, which WebKitGTK breaks.
  const indexAt = (clientY: number) => {
    const items = Array.from(listRef.current?.children ?? []);
    const index = items.findIndex((el) => {
      const r = el.getBoundingClientRect();
      return clientY < r.top + r.height / 2;
    });
    return index === -1 ? items.length - 1 : index;
  };

  return (
    <ol ref={listRef} className="nodrag space-y-1 px-2.5 py-2">
      {ids.map((nodeId, i) => {
        const { title, kind } = titleOf(nodeId);
        const target = drag && drag.from !== drag.to && drag.to === i;
        return (
          <li
            key={nodeId}
            className={`group/item flex items-center gap-1.5 rounded-lg px-1.5 py-1 text-xs ${
              drag?.from === i ? "bg-accent-soft" : "bg-sunken"
            } ${target ? "ring-1 ring-accent" : ""}`}
          >
            {reorderable ? (
              <button
                type="button"
                tabIndex={-1}
                aria-label={t("composer.node.reorder", { title })}
                className="cursor-grab touch-none text-text-subtle active:cursor-grabbing"
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture(e.pointerId);
                  setDrag({ from: i, to: i });
                }}
                onPointerMove={(e) => {
                  if (drag) setDrag({ ...drag, to: indexAt(e.clientY) });
                }}
                onPointerUp={() => {
                  if (drag) onReorder(drag.from, drag.to);
                  setDrag(null);
                }}
                onPointerCancel={() => setDrag(null)}
              >
                <GripVertical size={13} />
              </button>
            ) : null}
            <span className="w-4 shrink-0 text-right font-mono text-[10px] text-text-subtle">
              {i + 1}
            </span>
            <span
              className="size-1.5 shrink-0 rounded-full"
              style={{ background: kind ? kindColor(kind) : "var(--c-border-strong)" }}
            />
            <span className="min-w-0 flex-1 truncate">{title}</span>
            <button
              type="button"
              aria-label={t("composer.node.disconnectItem", { title })}
              title={t("composer.node.disconnect")}
              onClick={() => onDisconnect(nodeId)}
              className="shrink-0 rounded p-0.5 text-text-subtle opacity-0 group-hover/item:opacity-100 hover:bg-hover hover:text-danger focus-visible:opacity-100"
            >
              <X size={12} />
            </button>
          </li>
        );
      })}
    </ol>
  );
}

export const nodeTypes = { block: BlockNodeView, output: OutputNodeView };
