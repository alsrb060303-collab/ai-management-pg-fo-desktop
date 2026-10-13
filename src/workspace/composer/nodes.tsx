import { isTauri } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Handle, type NodeProps, Position, useReactFlow } from "@xyflow/react";
import { GripVertical, PencilLine, X } from "lucide-react";
import { memo, useRef, useState } from "react";
import { Switch } from "../../components/ui";
import { useT } from "../../i18n";
import {
  AGENTS_MD_BYTE_BUDGET,
  AGENTS_MD_BYTE_LIMIT,
  accepts,
  type Block,
  type BlockData,
  type BlockNode,
  CLAUDE_MD_LINE_BUDGET,
  EVENTS_WITHOUT_MATCHER,
  hookEvents,
  inputsOf,
  MODELS,
  type OutputData,
  type OutputNode,
  useComposerStore,
  useComposerTool,
  utf8Bytes,
} from "../../store/composer";
import { KIND_ICON, KIND_LABEL, kindColor, TARGETS, targetTitle } from "./kinds";
import { DeleteNodeButton, useDraft } from "./nodeParts";
import { PlanNodeView } from "./PlanNode";

export const BlockNodeView = memo(function BlockNodeView({
  id,
  data,
  selected,
}: NodeProps<BlockNode>) {
  const block = useComposerStore((s) => s.blocks.get(data.blockId));
  const setBlockData = useComposerStore((s) => s.setBlockData);
  const kind = block?.kind ?? "instruction";
  const on = !data.disabled;
  const Icon = KIND_ICON[kind];
  const color = kindColor(kind);
  const tool = useComposerTool();
  const t = useT();

  return (
    <div
      className={`group w-60 rounded-xl border bg-surface-raised shadow-card transition-colors ${
        selected ? "border-accent ring-2 ring-accent-soft" : "border-border"
      } ${on ? "" : "border-dashed"}`}
    >
      <div className="h-1 rounded-t-xl" style={{ background: color }} />
      <div className="px-3 pt-2 pb-2.5">
        <div className="flex items-center gap-1.5 text-fs-11 font-medium" style={{ color }}>
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
            <span
              className="nodrag flex scale-75"
              title={t(on ? "composer.node.onHint" : "composer.node.offHint")}
            >
              <Switch
                checked={on}
                label={t("composer.node.toggle")}
                onChange={(next) => setBlockData(id, { disabled: next ? undefined : true })}
              />
            </span>
          </span>
        </div>
        <div className={on ? "" : "opacity-45"}>
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
          {/* Model aliases are Claude Code's; Codex subagents leave them out. */}
          {block?.kind === "agent" && tool === "claude" && (
            <ModelPicker id={id} data={data} block={block} />
          )}
          {block?.kind === "hook" && <HookFields id={id} data={data} block={block} />}
        </div>
        {!on && <p className="mt-1.5 text-fs-11 text-text-subtle">{t("composer.node.off")}</p>}
      </div>
      <Handle
        type="source"
        position={Position.Right}
        style={{ background: color, borderColor: "var(--c-raised)" }}
      />
    </div>
  );
});

const fieldClass =
  "nodrag nowheel w-full rounded-md border border-border bg-surface px-1.5 py-1 text-fs-11 outline-none focus:border-accent";

/** Subagents: which model runs it. "Auto" keeps what the document says. */
function ModelPicker({ id, data, block }: { id: string; data: BlockData; block: Block }) {
  const setBlockData = useComposerStore((s) => s.setBlockData);
  const t = useT();
  const own = block.model?.trim() || null;

  const choose = async (next: string) => {
    if (next === (data.model ?? "")) return;
    if (!data.model && next) {
      const label = MODELS.find((m) => m.alias === next)?.label ?? next;
      const message = own
        ? t("composer.model.warnOwn", { title: block.title, model: own, next: label })
        : t("composer.model.warnAuto", { title: block.title, next: label });
      const ok = isTauri()
        ? await ask(message, { title: t("composer.model.warnTitle"), kind: "warning" }).catch(
            () => false,
          )
        : window.confirm(message);
      if (!ok) return;
    }
    setBlockData(id, { model: next || undefined });
  };

  return (
    <label className="nodrag mt-2 flex items-center gap-1.5 text-fs-11 text-text-muted">
      <span className="shrink-0">{t("composer.model.label")}</span>
      <select
        value={data.model ?? ""}
        onChange={(e) => void choose(e.target.value)}
        title={t("composer.model.hint")}
        className={`${fieldClass} flex-1`}
      >
        <option value="">
          {own ? t("composer.model.autoWith", { model: own }) : t("composer.model.auto")}
        </option>
        {MODELS.map((m) => (
          <option key={m.alias} value={m.alias}>
            {m.label}
          </option>
        ))}
      </select>
    </label>
  );
}

/** Hooks: when it runs (event, matcher) and the command it runs. */
function HookFields({ id, data, block }: { id: string; data: BlockData; block: Block }) {
  const setBlockData = useComposerStore((s) => s.setBlockData);
  const tool = useComposerTool();
  const t = useT();
  const event = data.event ?? block.event ?? "";
  const offered = hookEvents(tool);
  // An event the tool doesn't have stays selectable so the warning makes sense.
  const events = offered.includes(event) || !event ? offered : [event, ...offered];
  const usesMatcher = !EVENTS_WITHOUT_MATCHER.has(event);
  const matcher = useDraft(data.matcher ?? block.matcher ?? "", (next) =>
    setBlockData(id, { matcher: next }),
  );
  const command = (data.body ?? block.body).trim();

  return (
    <div className="nodrag mt-2 space-y-1.5 text-fs-11 text-text-muted">
      <label className="flex items-center gap-1.5">
        <span className="w-12 shrink-0">{t("composer.hook.event")}</span>
        <select
          value={event}
          onChange={(e) => setBlockData(id, { event: e.target.value })}
          className={`${fieldClass} flex-1`}
        >
          {!event && <option value="">{t("composer.hook.pickEvent")}</option>}
          {events.map((e) => (
            <option key={e} value={e}>
              {e}
            </option>
          ))}
        </select>
      </label>
      {usesMatcher && (
        <label className="flex items-center gap-1.5">
          <span className="w-12 shrink-0">{t("composer.hook.matcher")}</span>
          <input
            {...matcher}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            placeholder={t(
              tool === "claude" ? "composer.hook.matcherHint" : "composer.hook.matcherHintAny",
            )}
            spellCheck={false}
            className={`${fieldClass} flex-1 font-mono`}
          />
        </label>
      )}
      <div
        className={`truncate rounded-md bg-sunken px-1.5 py-1 font-mono ${command ? "" : "text-warning"}`}
        title={command || undefined}
      >
        {command ? `$ ${command}` : t("composer.hook.noCommand")}
      </div>
    </div>
  );
}

/** Custom outputs: the file's path and the user's own text. */
function CustomFields({ id, data }: { id: string; data: OutputData }) {
  const setOutputData = useComposerStore((s) => s.setOutputData);
  const t = useT();
  const path = useDraft(data.path ?? "", (next) => setOutputData(id, { path: next.trim() }));
  const text = useDraft(data.text ?? "", (next) => setOutputData(id, { text: next }));
  return (
    <div className="nodrag space-y-1.5 border-b border-border px-3.5 py-2.5 text-fs-11 text-text-muted">
      <label className="block">
        <span className="mb-0.5 block">{t("composer.custom.path")}</span>
        <input
          {...path}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          placeholder="AGENTS.md"
          spellCheck={false}
          className={`${fieldClass} font-mono`}
        />
      </label>
      <label className="block">
        <span className="mb-0.5 block">{t("composer.custom.text")}</span>
        <textarea
          {...text}
          rows={4}
          placeholder={t("composer.custom.textHint")}
          className={`${fieldClass} resize-y font-mono leading-snug`}
        />
      </label>
    </div>
  );
}

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
  const tool = useComposerTool();
  const { deleteElements } = useReactFlow();
  const t = useT();

  const personal = data.scope === "personal";
  const meta = TARGETS.find((m) => m.target === data.target) ?? TARGETS[0];
  const Icon = meta.icon;
  const inputs = doc ? inputsOf(doc, id) : [];
  const isInstructions = data.target === "instructions";
  // Claude Code: CLAUDE.md is followed better under a line budget. Codex:
  // AGENTS.md is cut after a byte limit.
  const budget =
    tool === "codex"
      ? {
          used: isInstructions && file ? utf8Bytes(file.content) : 0,
          max: AGENTS_MD_BYTE_LIMIT,
          warnAt: AGENTS_MD_BYTE_BUDGET,
        }
      : {
          used: isInstructions ? (file?.lines ?? 0) : 0,
          max: CLAUDE_MD_LINE_BUDGET,
          warnAt: CLAUDE_MD_LINE_BUDGET,
        };
  const over = budget.used > budget.warnAt;
  const kib = (bytes: number) => (bytes / 1024).toFixed(1).replace(/\.0$/, "");

  const titleOf = (nodeId: string) => {
    const node = doc?.nodes.find((n) => n.id === nodeId);
    if (node?.type === "plan")
      return {
        title: node.data.title || t("composer.plan.main"),
        kind: "plan" as const,
        off: !!node.data.disabled,
      };
    if (node?.type !== "block") return { title: nodeId, kind: undefined };
    const block = blocks.get(node.data.blockId);
    return {
      title: block?.title ?? node.data.blockId,
      kind: block?.kind,
      off: !!node.data.disabled,
    };
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
          <div className="truncate font-mono text-fs-13 font-semibold">
            {data.target === "custom" && !data.path?.trim()
              ? t("composer.target.custom")
              : targetTitle(data.target, personal, tool, data.path)}
          </div>
          <div className="text-fs-11 text-text-subtle">
            {t("composer.node.accepts", {
              kinds:
                data.target === "plans"
                  ? t("composer.plan.main")
                  : accepts(data.target, tool)
                      .map((k) => t(KIND_LABEL[k]))
                      .join(" · "),
            })}
          </div>
        </div>
        <fieldset
          aria-label={t("composer.node.scope")}
          className="nodrag flex rounded-md bg-sunken p-0.5 text-fs-11"
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
              {scope === "team" ? t("composer.node.team") : t("composer.node.personal")}
            </button>
          ))}
        </fieldset>
        <span className={selected ? "[&>button]:opacity-100" : ""}>
          <DeleteNodeButton id={id} label={t("composer.node.deleteOutput")} />
        </span>
      </div>

      {data.target === "custom" && <CustomFields id={id} data={data} />}

      {inputs.length === 0 ? (
        <p className="px-3.5 py-4 text-center text-xs text-text-subtle">
          {data.target === "custom"
            ? t("composer.custom.empty")
            : data.target === "plans"
              ? t("composer.plan.outputEmpty")
              : t("composer.node.empty")}
        </p>
      ) : (
        <OrderedInputs
          ids={inputs}
          titleOf={titleOf}
          reorderable={isInstructions || data.target === "plans"}
          onReorder={(from, to) => reorder(id, from, to)}
          onDisconnect={(source) => {
            const edges = (doc?.edges ?? [])
              .filter((e) => e.source === source && e.target === id)
              .map((e) => ({ id: e.id }));
            void deleteElements({ edges });
          }}
        />
      )}

      <div className="flex items-center gap-2 border-t border-border px-3.5 py-2 text-fs-11 text-text-muted">
        {isInstructions ? (
          <>
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-sunken">
              <div
                className={`h-full rounded-full ${over ? "bg-danger" : "bg-accent"}`}
                style={{ width: `${Math.min(100, (budget.used / budget.max) * 100)}%` }}
              />
            </div>
            <span className={over ? "font-medium text-danger" : ""}>
              {tool === "codex"
                ? t("composer.node.byteBudget", { n: kib(budget.used), max: kib(budget.max) })
                : t("composer.node.lineBudget", { n: budget.used, max: budget.max })}
            </span>
          </>
        ) : (
          <span>{t("composer.node.fileCount", { n: fileCount ?? 0 })}</span>
        )}
      </div>
    </div>
  );
});

/** The output's connected blocks; the instructions' order can be changed by dragging. */
function OrderedInputs({
  ids,
  titleOf,
  reorderable,
  onReorder,
  onDisconnect,
}: {
  ids: string[];
  titleOf: (id: string) => {
    title: string;
    kind?: keyof typeof KIND_LABEL | "plan";
    off?: boolean;
  };
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
        const { title, kind, off } = titleOf(nodeId);
        const target = drag && drag.from !== drag.to && drag.to === i;
        return (
          <li
            key={nodeId}
            className={`group/item flex items-center gap-1.5 rounded-lg px-1.5 py-1 text-xs ${
              drag?.from === i ? "bg-accent-soft" : "bg-sunken"
            } ${target ? "ring-1 ring-accent" : ""} ${off ? "opacity-50" : ""}`}
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
            <span className="w-4 shrink-0 text-right font-mono text-fs-10 text-text-subtle">
              {i + 1}
            </span>
            <span
              className="size-1.5 shrink-0 rounded-full"
              style={{ background: kind ? kindColor(kind) : "var(--c-border-strong)" }}
            />
            <span className={`min-w-0 flex-1 truncate ${off ? "line-through" : ""}`}>{title}</span>
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

export const nodeTypes = { block: BlockNodeView, output: OutputNodeView, plan: PlanNodeView };
