import { type NodeProps, NodeResizer } from "@xyflow/react";
import { Group, StickyNote } from "lucide-react";
import { memo } from "react";
import { useT } from "../../i18n";
import { type GroupNode, type MemoNode, useComposerStore } from "../../store/composer";
import { DeleteNodeButton, useDraft } from "./nodeParts";

/** A note on the canvas: no handles, never compiled or written. */
export const MemoNodeView = memo(function MemoNodeView({
  id,
  data,
  selected,
}: NodeProps<MemoNode>) {
  const setMemo = useComposerStore((s) => s.setMemo);
  const text = useDraft(data.text, (next) => setMemo(id, next));
  const t = useT();
  return (
    <div
      className={`group w-56 rounded-xl border bg-memo shadow-card ${
        selected ? "border-accent ring-2 ring-accent-soft" : "border-memo-border"
      }`}
    >
      <div className="flex items-center gap-1.5 px-3 pt-2 text-fs-11 font-medium text-warning">
        <StickyNote size={12} />
        {t("composer.memo.label")}
        <span className="ml-auto text-text-subtle" title={t("composer.palette.memoHint")}>
          {t("composer.memo.offFile")}
        </span>
        <span className={selected ? "[&>button]:opacity-100" : ""}>
          <DeleteNodeButton id={id} label={t("composer.memo.delete")} />
        </span>
      </div>
      <textarea
        {...text}
        data-memo={id}
        aria-label={t("composer.memo.label")}
        placeholder={t("composer.memo.placeholder")}
        rows={4}
        className="nodrag nowheel block w-full resize-y bg-transparent px-3 pt-1 pb-2.5 text-xs leading-relaxed outline-none placeholder:text-text-subtle"
      />
    </div>
  );
});

/** A frame around nodes; dragging it moves them along. */
export const GroupNodeView = memo(function GroupNodeView({
  id,
  data,
  selected,
}: NodeProps<GroupNode>) {
  const setGroupTitle = useComposerStore((s) => s.setGroupTitle);
  const checkpoint = useComposerStore((s) => s.checkpoint);
  const t = useT();
  const title = useDraft(data.title, (next) =>
    setGroupTitle(id, next.trim() || t("composer.group.untitled")),
  );
  return (
    <div
      className={`group h-full w-full rounded-2xl border-2 border-dashed transition-colors ${
        selected ? "border-accent bg-accent-soft" : "border-border-strong bg-hover"
      }`}
    >
      <NodeResizer
        isVisible={selected}
        minWidth={160}
        minHeight={80}
        onResizeStart={checkpoint}
        color="var(--c-accent)"
      />
      <div className="flex items-center gap-1.5 px-3 pt-1.5 text-text-muted">
        <Group size={14} className="shrink-0" />
        <input
          {...title}
          data-group-title={id}
          aria-label={t("composer.group.name")}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          className="nodrag min-w-0 flex-1 rounded-md bg-transparent px-1 py-0.5 text-fs-13 font-semibold text-text outline-none hover:bg-hover focus:bg-surface-raised"
        />
        <span className={selected ? "[&>button]:opacity-100" : ""}>
          <DeleteNodeButton id={id} label={t("composer.group.delete")} />
        </span>
      </div>
    </div>
  );
});
