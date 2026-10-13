import { Handle, type NodeProps, Position } from "@xyflow/react";
import { CornerDownRight, CornerUpLeft, Flag, GripVertical, Plus, X } from "lucide-react";
import { memo, useRef, useState } from "react";
import { Switch } from "../../components/ui";
import { useT } from "../../i18n";
import {
  type PlanAction,
  type PlanItem,
  type PlanNode,
  type PlanStatus,
  planGroupSize,
  useComposerStore,
} from "../../store/composer";
import { kindColor } from "./kinds";
import { DeleteNodeButton, useDraft } from "./nodeParts";
import { ACTIONS, dropIndex, layout, nextStatus, STATUS, setPlanDrop, usePlanDrop } from "./plans";

const color = kindColor("plan");
const field =
  "nodrag nowheel min-w-0 flex-1 rounded-md border border-transparent bg-transparent px-1.5 py-0.5 outline-none hover:border-border focus:border-accent focus:bg-surface";

/**
 * A main goal: its sub-goals, their detail goals, and steps between them, in
 * one larger node. Rows are dragged into place; a detail goal always sits
 * under a sub-goal.
 */
export const PlanNodeView = memo(function PlanNodeView({
  id,
  data,
  selected,
}: NodeProps<PlanNode>) {
  const t = useT();
  const { setPlan, addPlanItem, movePlanItem } = useComposerStore.getState();
  const on = !data.disabled;
  const title = useDraft(data.title, (title) => setPlan(id, { title }));
  const note = useDraft(data.note ?? "", (note) => setPlan(id, { note: note || undefined }));
  const listRef = useRef<HTMLOListElement>(null);
  /** The row being dragged by its grip; with a sub-goal, its whole group moves. */
  const [drag, setDrag] = useState<{ from: number; size: number } | null>(null);
  const gap = usePlanDrop((d) => (d?.node === id ? d.gap : null));
  // Dropping into the dragged group, or right beside it, changes nothing.
  const line =
    gap === null || (drag && gap >= drag.from && gap <= drag.from + drag.size) ? null : gap;
  const rows = layout(data.items);
  const finish = (move: boolean) => {
    const to = usePlanDrop.getState();
    if (move && drag && to?.node === id) movePlanItem(id, drag.from, to.gap);
    setDrag(null);
    setPlanDrop(null);
  };

  return (
    <div
      data-plan-node={id}
      className={`group w-[26rem] rounded-2xl border-2 bg-surface-raised shadow-card transition-colors ${
        selected ? "border-accent ring-2 ring-accent-soft" : "border-border-strong"
      } ${on ? "" : "border-dashed"}`}
    >
      <div className="h-1.5 rounded-t-2xl" style={{ background: color }} />
      <div className="px-3.5 pt-2.5 pb-3">
        <div className="flex items-center gap-1.5 text-[11px] font-semibold" style={{ color }}>
          <Flag size={12} />
          {t("composer.plan.main")}
          <span className="ml-auto flex items-center gap-1">
            <span className={selected ? "[&>button]:opacity-100" : ""}>
              <DeleteNodeButton id={id} label={t("composer.plan.delete")} />
            </span>
            <span
              className="nodrag flex scale-75"
              title={t(on ? "composer.node.onHint" : "composer.node.offHint")}
            >
              <Switch
                checked={on}
                label={t("composer.node.toggle")}
                onChange={(next) => setPlan(id, { disabled: next ? undefined : true })}
              />
            </span>
          </span>
        </div>

        <div className={on ? "" : "opacity-45"}>
          <div className="mt-1.5 flex items-center gap-1.5">
            <StatusButton
              status={data.status}
              onChange={(status) => setPlan(id, { status })}
              size={18}
            />
            <input
              {...title}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder={t("composer.plan.titlePlaceholder")}
              aria-label={t("composer.plan.main")}
              className={`${field} text-[15px] font-semibold`}
            />
          </div>
          <textarea
            {...note}
            rows={1}
            placeholder={t("composer.plan.note")}
            aria-label={t("composer.plan.note")}
            className={`${field} mt-1 ml-6 block w-[calc(100%-1.5rem)] resize-none text-xs text-text-muted`}
          />

          {data.items.length === 0 ? (
            <p
              className={`mt-3 rounded-lg border border-dashed px-3 py-4 text-center text-xs text-text-subtle ${
                line === 0 ? "border-drop" : "border-border"
              }`}
            >
              {t("composer.plan.empty")}
            </p>
          ) : (
            <ol ref={listRef} className="nodrag mt-2.5 space-y-1">
              {data.items.map((item, i) => (
                <PlanRow
                  key={item.id}
                  nodeId={id}
                  item={item}
                  index={i}
                  number={rows[i].number}
                  level={rows[i].level}
                  hasSubAbove={data.items.slice(0, i).some((x) => x.kind === "sub")}
                  dragging={!!drag && i >= drag.from && i < drag.from + drag.size}
                  line={
                    line === i
                      ? "before"
                      : line === data.items.length && i === line - 1
                        ? "after"
                        : null
                  }
                  onGrab={() => {
                    setDrag({ from: i, size: planGroupSize(data.items, i) });
                    setPlanDrop({ node: id, gap: i });
                  }}
                  onMove={(y) => {
                    if (drag && listRef.current)
                      setPlanDrop({ node: id, gap: dropIndex(listRef.current, y) });
                  }}
                  onDrop={() => finish(true)}
                  onCancel={() => finish(false)}
                />
              ))}
            </ol>
          )}

          <div className="nodrag mt-2.5 flex flex-wrap gap-1">
            {(["sub", "detail", "step"] as const).map((kind) => (
              <button
                key={kind}
                type="button"
                onClick={() => addPlanItem(id, { kind })}
                className="flex items-center gap-0.5 rounded-md border border-border px-1.5 py-0.5 text-[11px] text-text-muted hover:border-border-strong hover:text-text"
              >
                <Plus size={11} />
                {t(`composer.plan.${kind}`)}
              </button>
            ))}
          </div>
        </div>
        {!on && <p className="mt-1.5 text-[11px] text-text-subtle">{t("composer.node.off")}</p>}
      </div>
      <Handle
        type="source"
        position={Position.Right}
        style={{ background: color, borderColor: "var(--c-raised)" }}
      />
    </div>
  );
});

function StatusButton({
  status,
  onChange,
  size = 15,
}: {
  status: PlanStatus | undefined;
  onChange: (s: PlanStatus) => void;
  size?: number;
}) {
  const t = useT();
  const s = STATUS[status ?? "n"];
  const Icon = s.icon;
  return (
    <button
      type="button"
      onClick={() => onChange(nextStatus(status))}
      title={`${t(s.label)} · ${t("composer.plan.statusHint")}`}
      aria-label={t(s.label)}
      className={`nodrag shrink-0 rounded-full hover:opacity-75 ${s.tone}`}
    >
      <Icon size={size} />
    </button>
  );
}

function PlanRow({
  nodeId,
  item,
  index,
  number,
  level,
  hasSubAbove,
  dragging,
  line,
  onGrab,
  onMove,
  onDrop,
  onCancel,
}: {
  nodeId: string;
  item: PlanItem;
  index: number;
  number: string;
  level: number;
  hasSubAbove: boolean;
  dragging: boolean;
  /** The drop line, drawn in the gap above or below this row. */
  line: "before" | "after" | null;
  onGrab: () => void;
  onMove: (clientY: number) => void;
  onDrop: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const { setPlanItem, removePlanItem } = useComposerStore.getState();
  const text = useDraft(item.text, (next) => setPlanItem(nodeId, item.id, { text: next }));
  const step = item.kind === "step";
  const label = step
    ? t("composer.plan.step")
    : t(item.kind === "sub" ? "composer.plan.sub" : "composer.plan.detail");

  return (
    <li
      data-plan-row={index}
      style={{ marginLeft: level * 18 }}
      className={`group/row relative flex items-start gap-1 rounded-lg px-1 py-1 text-xs ${
        step
          ? "border border-dashed bg-surface"
          : item.kind === "sub"
            ? "bg-sunken font-medium"
            : "bg-sunken/60"
      } ${dragging ? "opacity-50 ring-1 ring-accent" : ""}`}
    >
      {line && (
        <span
          aria-hidden
          className={`pointer-events-none absolute -inset-x-1 h-0.5 rounded-full bg-drop ${
            line === "before" ? "-top-[3px]" : "-bottom-[3px]"
          }`}
        />
      )}
      <button
        type="button"
        tabIndex={-1}
        aria-label={t("composer.plan.move", { title: item.text || label })}
        title={t("composer.plan.moveHint")}
        className="mt-0.5 cursor-grab touch-none text-text-subtle active:cursor-grabbing"
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          onGrab();
        }}
        onPointerMove={(e) => onMove(e.clientY)}
        onPointerUp={onDrop}
        onPointerCancel={onCancel}
      >
        <GripVertical size={13} />
      </button>

      {step ? (
        <StepFields nodeId={nodeId} item={item} text={text} />
      ) : (
        <>
          <StatusButton
            status={item.status}
            onChange={(status) => setPlanItem(nodeId, item.id, { status })}
          />
          <span className="mt-px w-6 shrink-0 font-mono text-[10px] text-text-subtle">
            {number}
          </span>
          <input
            {...text}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            placeholder={label}
            aria-label={label}
            className={field}
          />
          {item.kind === "sub" && hasSubAbove && (
            <RowButton
              label={t("composer.plan.toDetail")}
              onClick={() => setPlanItem(nodeId, item.id, { kind: "detail" })}
            >
              <CornerDownRight size={12} />
            </RowButton>
          )}
          {item.kind === "detail" && (
            <RowButton
              label={t("composer.plan.toSub")}
              onClick={() => setPlanItem(nodeId, item.id, { kind: "sub" })}
            >
              <CornerUpLeft size={12} />
            </RowButton>
          )}
        </>
      )}
      <RowButton label={t("composer.plan.remove")} onClick={() => removePlanItem(nodeId, item.id)}>
        <X size={12} />
      </RowButton>
    </li>
  );
}

function RowButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className="mt-px shrink-0 rounded p-0.5 text-text-subtle opacity-0 group-hover/row:opacity-100 hover:bg-hover hover:text-text focus-visible:opacity-100"
    >
      {children}
    </button>
  );
}

/** The block picker's "type a name" choice; catalog ids never look like this. */
const TYPED = "__typed__";

/** A step: what kind, which subagent or skill (picked or typed), and the text. */
function StepFields({
  nodeId,
  item,
  text,
}: {
  nodeId: string;
  item: PlanItem;
  text: ReturnType<typeof useDraft>;
}) {
  const t = useT();
  const catalog = useComposerStore((s) => s.catalog);
  const setPlanItem = useComposerStore((s) => s.setPlanItem);
  const action = item.action ?? "text";
  const kind = action === "agent" ? "agent" : action === "skill" ? "skill" : null;
  const options = kind ? catalog.filter((b) => b.kind === kind) : [];
  const typed = item.name !== undefined;
  const name = useDraft(item.name ?? "", (next) =>
    setPlanItem(nodeId, item.id, { name: next.trim() }),
  );
  const select =
    "nodrag nowheel rounded-md border border-border bg-surface px-1 py-0.5 text-[11px] outline-none focus:border-accent";

  return (
    <div className="min-w-0 flex-1 space-y-1">
      <div className="flex flex-wrap items-center gap-1">
        <span
          className="rounded px-1 py-px text-[10px] font-semibold text-white"
          style={{ background: color }}
        >
          {t("composer.plan.step")}
        </span>
        <select
          value={action}
          aria-label={t("composer.plan.actionLabel")}
          onChange={(e) =>
            setPlanItem(nodeId, item.id, {
              action: e.target.value as PlanAction,
              block: undefined,
              name: undefined,
            })
          }
          className={select}
        >
          {ACTIONS.map((a) => (
            <option key={a.action} value={a.action}>
              {t(a.label)}
            </option>
          ))}
        </select>
        {kind && (
          <select
            value={typed ? TYPED : (item.block ?? "")}
            aria-label={t(kind === "agent" ? "composer.plan.pickAgent" : "composer.plan.pickSkill")}
            onChange={(e) => {
              const value = e.target.value;
              setPlanItem(
                nodeId,
                item.id,
                value === TYPED
                  ? { block: undefined, name: item.name ?? "" }
                  : { block: value || undefined, name: undefined },
              );
            }}
            className={`${select} max-w-44`}
          >
            <option value="">
              {t(kind === "agent" ? "composer.plan.pickAgent" : "composer.plan.pickSkill")}
            </option>
            {options.map((b) => (
              <option key={b.id} value={b.id}>
                {b.title}
              </option>
            ))}
            <option value={TYPED}>{t("composer.plan.typeName")}</option>
          </select>
        )}
      </div>
      {kind && typed && (
        <input
          {...name}
          // biome-ignore lint/a11y/noAutofocus: shown right after picking "type a name"
          autoFocus={!item.name}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          placeholder={t(kind === "agent" ? "composer.plan.agentName" : "composer.plan.skillName")}
          aria-label={t(kind === "agent" ? "composer.plan.agentName" : "composer.plan.skillName")}
          className={`${field} block w-full font-mono text-[11px]`}
        />
      )}
      <textarea
        {...text}
        rows={1}
        placeholder={t(
          action === "text"
            ? "composer.plan.stepText"
            : action === "ask"
              ? "composer.plan.askText"
              : "composer.plan.handText",
        )}
        aria-label={t("composer.plan.step")}
        className={`${field} block w-full resize-none`}
      />
    </div>
  );
}
