import { useReactFlow, type XYPosition } from "@xyflow/react";
import {
  ChevronRight,
  Copy,
  Eye,
  FilePlus,
  Flag,
  Group,
  type LucideIcon,
  Maximize,
  PanelRight,
  PencilLine,
  Power,
  PowerOff,
  Redo2,
  SquareDashedMousePointer,
  SquarePlus,
  StickyNote,
  Trash2,
  Undo2,
  Ungroup,
  Upload,
} from "lucide-react";
import { type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useT } from "../../i18n";
import { targetAvailable, useComposerStore, useComposerTool } from "../../store/composer";
import { instructionsLabel, TARGETS } from "./kinds";

/** Where the menu opened, and the nodes it is about (none: the empty canvas). */
export interface MenuAt {
  x: number;
  y: number;
  flow: XYPosition;
  nodeIds: string[];
}

interface Item {
  label: string;
  icon?: LucideIcon;
  shortcut?: string;
  danger?: boolean;
  disabled?: boolean;
  onSelect?: () => void;
  children?: Item[];
}
type Entry = Item | "sep" | { heading: string };

const MENU_WIDTH = 232;

export function CanvasMenu({
  at,
  preview,
  onShowPreview,
  onTogglePreview,
  onWrite,
  onClose,
}: {
  at: MenuAt;
  preview: boolean;
  onShowPreview: () => void;
  onTogglePreview: () => void;
  onWrite: () => void;
  onClose: () => void;
}) {
  const entries = useEntries(at, { preview, onShowPreview, onTogglePreview, onWrite });
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: at.x, y: at.y });

  // Keep it on screen.
  useLayoutEffect(() => {
    const rect = ref.current?.getBoundingClientRect();
    if (!rect) return;
    setPos({
      x: Math.max(4, Math.min(at.x, window.innerWidth - rect.width - 4)),
      y: Math.max(4, Math.min(at.y, window.innerHeight - rect.height - 4)),
    });
  }, [at.x, at.y]);

  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    const away = (e: Event) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const close = () => onClose();
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("wheel", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("wheel", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [onClose]);

  const openLeft = pos.x + MENU_WIDTH * 2 > window.innerWidth;

  return createPortal(
    <div
      ref={ref}
      role="menu"
      className="fixed z-50"
      style={{ left: pos.x, top: pos.y, width: MENU_WIDTH }}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => menuKeys(e, onClose)}
    >
      <MenuList entries={entries} onClose={onClose} openLeft={openLeft} />
    </div>,
    document.body,
  );
}

/** Arrow keys move between items, Escape closes, ArrowLeft leaves a submenu. */
function menuKeys(e: KeyboardEvent<HTMLElement>, onClose: () => void) {
  if (e.key === "Escape") {
    e.preventDefault();
    onClose();
    return;
  }
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  e.preventDefault();
  const list = (e.target as HTMLElement).closest("[data-menu-list]");
  if (!list) return;
  const items = Array.from(
    list.querySelectorAll<HTMLButtonElement>(":scope > li > button:not(:disabled)"),
  );
  const index = items.indexOf(e.target as HTMLButtonElement);
  const next = e.key === "ArrowDown" ? index + 1 : index - 1;
  items[(next + items.length) % items.length]?.focus();
}

function MenuList({
  entries,
  onClose,
  openLeft,
}: {
  entries: Entry[];
  onClose: () => void;
  openLeft: boolean;
}) {
  const [open, setOpen] = useState<number | null>(null);
  return (
    <ul
      data-menu-list
      className="rounded-xl border border-border bg-surface-raised p-1 text-fs-13 shadow-card"
    >
      {entries.map((entry, i) => {
        if (entry === "sep") {
          // biome-ignore lint/suspicious/noArrayIndexKey: separators have nothing else to key on
          return <li key={`sep${i}`} aria-hidden className="my-1 h-px bg-border" />;
        }
        if ("heading" in entry) {
          return (
            <li key={entry.heading} className="px-2 pt-1 pb-0.5 text-fs-11 text-text-subtle">
              {entry.heading}
            </li>
          );
        }
        const Icon = entry.icon;
        const sub = entry.children;
        return (
          <li
            key={entry.label}
            className="relative"
            onPointerEnter={() => setOpen(sub ? i : null)}
            onKeyDown={(e) => {
              if (sub && (e.key === "ArrowRight" || e.key === "Enter")) {
                e.preventDefault();
                e.stopPropagation();
                setOpen(i);
                requestAnimationFrame(() =>
                  e.currentTarget
                    .querySelector<HTMLButtonElement>("[data-menu-list] button")
                    ?.focus(),
                );
              } else if (e.key === "ArrowLeft" && open === i) {
                e.preventDefault();
                e.stopPropagation();
                setOpen(null);
                (e.currentTarget.firstElementChild as HTMLElement | null)?.focus();
              }
            }}
          >
            <button
              type="button"
              role="menuitem"
              aria-haspopup={sub ? "menu" : undefined}
              aria-expanded={sub ? open === i : undefined}
              disabled={entry.disabled}
              onClick={() => {
                if (sub) {
                  setOpen(i);
                  return;
                }
                onClose();
                entry.onSelect?.();
              }}
              className={`flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left outline-none disabled:opacity-40 ${
                entry.danger
                  ? "text-danger hover:bg-danger-soft focus-visible:bg-danger-soft"
                  : "hover:bg-hover focus-visible:bg-hover"
              }`}
            >
              <span className="flex size-4 shrink-0 items-center justify-center text-text-muted">
                {Icon && <Icon size={14} className={entry.danger ? "text-danger" : ""} />}
              </span>
              <span className="min-w-0 flex-1 truncate">{entry.label}</span>
              {entry.shortcut && (
                <span className="shrink-0 text-fs-11 text-text-subtle">{entry.shortcut}</span>
              )}
              {sub && <ChevronRight size={13} className="shrink-0 text-text-subtle" />}
            </button>
            {sub && open === i && (
              <div
                className={`absolute -top-1 ${openLeft ? "right-full mr-1" : "left-full ml-1"}`}
                style={{ width: MENU_WIDTH - 40 }}
              >
                <MenuList entries={sub} onClose={onClose} openLeft={openLeft} />
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function useEntries(
  at: MenuAt,
  app: {
    preview: boolean;
    onShowPreview: () => void;
    onTogglePreview: () => void;
    onWrite: () => void;
  },
): Entry[] {
  const store = useComposerStore.getState();
  const doc = useComposerStore((s) => s.doc);
  const canUndo = useComposerStore((s) => s.past.length > 0);
  const canRedo = useComposerStore((s) => s.future.length > 0);
  const tool = useComposerTool();
  const flow = useReactFlow();
  const t = useT();

  const nodes = (doc?.nodes ?? []).filter((n) => at.nodeIds.includes(n.id));
  const ids = nodes.map((n) => n.id);
  const entries: Entry[] = [];

  if (nodes.length === 0) {
    entries.push(
      {
        label: t("composer.menu.addMemo"),
        icon: StickyNote,
        onSelect: () => store.addMemo(at.flow),
      },
      { label: t("composer.menu.addGoal"), icon: Flag, onSelect: () => store.addPlan(at.flow) },
      {
        label: t("composer.menu.addOutput"),
        icon: SquarePlus,
        children: TARGETS.filter((m) => targetAvailable(m.target, tool)).map(
          ({ target, label, icon }) => ({
            label: label ? t(label) : instructionsLabel(tool),
            icon,
            onSelect: () => store.addOutput(target, at.flow),
          }),
        ),
      },
    );
  } else {
    const single = nodes.length === 1 ? nodes[0] : null;
    if (!single) entries.push({ heading: t("composer.menu.selected", { n: nodes.length }) });

    if (single?.type === "block") {
      entries.push({
        label: t("composer.menu.edit"),
        icon: PencilLine,
        onSelect: () => {
          app.onShowPreview();
          store.editNode(single.id);
        },
      });
    }
    if (single?.type === "memo") {
      entries.push({
        label: t("composer.menu.edit"),
        icon: PencilLine,
        onSelect: () => focusField(`[data-memo="${single.id}"]`),
      });
    }
    if (single?.type === "group") {
      entries.push({
        label: t("composer.menu.renameGroup"),
        icon: PencilLine,
        onSelect: () => focusField(`[data-group-title="${single.id}"]`),
      });
    }
    if (single && (single.type === "block" || single.type === "plan" || single.type === "output")) {
      entries.push({
        label: t("composer.menu.preview"),
        icon: Eye,
        onSelect: () => {
          app.onShowPreview();
          void store.focusNode(single.id);
        },
      });
    }

    const switchable = nodes.filter((n) => n.type === "block" || n.type === "plan");
    if (single && switchable.length === 1) {
      const off = !!(single.data as { disabled?: boolean }).disabled;
      entries.push({
        label: t(off ? "composer.menu.turnOn" : "composer.menu.turnOff"),
        icon: off ? Power : PowerOff,
        onSelect: () => store.setDisabled(ids, !off),
      });
    } else if (switchable.length > 1) {
      const switchIds = switchable.map((n) => n.id);
      entries.push(
        {
          label: t("composer.menu.allOn"),
          icon: Power,
          onSelect: () => store.setDisabled(switchIds, false),
        },
        {
          label: t("composer.menu.allOff"),
          icon: PowerOff,
          onSelect: () => store.setDisabled(switchIds, true),
        },
      );
    }
    entries.push({
      label: t("composer.menu.duplicate"),
      icon: Copy,
      shortcut: "Ctrl+D",
      onSelect: () => store.duplicate(ids),
    });

    entries.push("sep");
    if (nodes.some((n) => n.type !== "group")) {
      entries.push({
        label: t("composer.menu.group"),
        icon: Group,
        shortcut: "Ctrl+G",
        onSelect: () => store.groupNodes(ids),
      });
    }
    if (nodes.some((n) => n.parentId)) {
      entries.push({
        label: t("composer.menu.leaveGroup"),
        icon: Ungroup,
        onSelect: () => store.ungroup(nodes.filter((n) => n.parentId).map((n) => n.id)),
      });
    }
    const groups = nodes.filter((n) => n.type === "group").map((n) => n.id);
    if (groups.length > 0) {
      entries.push(
        {
          label: t("composer.menu.dissolve"),
          icon: Ungroup,
          shortcut: "Ctrl+Shift+G",
          onSelect: () => store.ungroup(groups),
        },
        {
          label: t("composer.menu.deleteWithContents"),
          icon: Trash2,
          danger: true,
          onSelect: () => store.deleteWithContents(ids),
        },
      );
    }
    // Deleting only groups is the same as ungrouping (their blocks stay).
    if (groups.length < nodes.length)
      entries.push({
        label: t("composer.menu.delete"),
        icon: Trash2,
        shortcut: "Delete",
        danger: true,
        onSelect: () => void flow.deleteElements({ nodes: ids.map((id) => ({ id })) }),
      });
  }

  entries.push(
    "sep",
    {
      label: t("composer.menu.undo"),
      icon: Undo2,
      shortcut: "Ctrl+Z",
      disabled: !canUndo,
      onSelect: store.undo,
    },
    {
      label: t("composer.menu.redo"),
      icon: Redo2,
      shortcut: "Ctrl+Shift+Z",
      disabled: !canRedo,
      onSelect: store.redo,
    },
    {
      label: t("composer.menu.selectAll"),
      icon: SquareDashedMousePointer,
      shortcut: "Ctrl+A",
      onSelect: () => store.select("all"),
    },
    {
      label: t("composer.menu.fitView"),
      icon: Maximize,
      onSelect: () => void flow.fitView({ maxZoom: 1, padding: 0.2, duration: 200 }),
    },
    {
      label: app.preview ? t("composer.hidePreview") : t("composer.showPreview"),
      icon: PanelRight,
      onSelect: app.onTogglePreview,
    },
    { label: t("composer.menu.write"), icon: Upload, onSelect: app.onWrite },
    {
      label: t("composer.menu.newDoc"),
      icon: FilePlus,
      onSelect: () => void store.newDoc(),
    },
  );
  return entries;
}

/** Focuses a text field on the canvas once the menu has closed. */
function focusField(selector: string) {
  requestAnimationFrame(() => {
    const field = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(selector);
    field?.focus();
    field?.select();
  });
}
