import "@xyflow/react/dist/style.css";

import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
} from "@xyflow/react";
import { Check, CloudOff, Loader2, PanelRight, Redo2, Undo2 } from "lucide-react";
import { type MouseEvent as ReactMouseEvent, useCallback, useEffect, useState } from "react";
import { IconButton } from "../../components/ui";
import { useT } from "../../i18n";
import { useComposerStore } from "../../store/composer";
import { getPref, setPref } from "../../store/prefs";
import { useThemeStore } from "../../store/theme";
import { CanvasMenu, type MenuAt } from "./CanvasMenu";
import { nodeTypes } from "./nodes";
import { Palette } from "./Palette";
import { PreviewPanel } from "./PreviewPanel";
import { RatingPanel } from "./ratings";
import { WriteDialog } from "./WriteDialog";

const PREVIEW_KEY = "composer-preview";

function readPreviewPref(): boolean {
  return getPref(PREVIEW_KEY) !== "0";
}

export default function ComposerPage() {
  const init = useComposerStore((s) => s.init);
  const doc = useComposerStore((s) => s.doc);
  const [writing, setWriting] = useState(false);
  const [preview, setPreview] = useState(readPreviewPref);
  // Double-clicking a node opens the preview on its file.
  const focus = useComposerStore((s) => s.focus);
  useEffect(() => {
    if (focus) setPreview(true);
  }, [focus]);
  const t = useT();
  const togglePreview = () => {
    setPreview(!preview);
    setPref(PREVIEW_KEY, preview ? "0" : "1");
  };

  useEffect(() => {
    void init();
  }, [init]);

  useCanvasShortcuts();

  if (!doc) {
    return <div className="p-10 text-sm text-text-muted">{t("composer.loading")}</div>;
  }

  return (
    <ReactFlowProvider>
      <div className="flex h-full">
        <Palette />
        <div className="flex min-w-0 flex-1 flex-col">
          <Toolbar preview={preview} onTogglePreview={togglePreview} />
          <Canvas
            preview={preview}
            onShowPreview={() => setPreview(true)}
            onTogglePreview={togglePreview}
            onWrite={() => setWriting(true)}
          />
        </div>
        {preview && <PreviewPanel onWrite={() => setWriting(true)} />}
      </div>
      {writing && <WriteDialog doc={doc} onClose={() => setWriting(false)} />}
    </ReactFlowProvider>
  );
}

function Canvas(app: {
  preview: boolean;
  onShowPreview: () => void;
  onTogglePreview: () => void;
  onWrite: () => void;
}) {
  const doc = useComposerStore((s) => s.doc);
  const onNodesChange = useComposerStore((s) => s.onNodesChange);
  const onEdgesChange = useComposerStore((s) => s.onEdgesChange);
  const onConnect = useComposerStore((s) => s.onConnect);
  const isValidConnection = useComposerStore((s) => s.isValidConnection);
  const checkpoint = useComposerStore((s) => s.checkpoint);
  const blocks = useComposerStore((s) => s.blocks);
  const theme = useThemeStore((s) => s.resolved);
  const { screenToFlowPosition } = useReactFlow();
  const [menu, setMenu] = useState<MenuAt | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);

  const openMenu = (e: ReactMouseEvent | MouseEvent, nodeIds: string[]) => {
    e.preventDefault();
    const at = { x: e.clientX, y: e.clientY };
    setMenu({ ...at, flow: screenToFlowPosition(at), nodeIds });
  };

  if (!doc) return null;
  return (
    <div className="min-h-0 flex-1">
      <ReactFlow
        key={doc.id}
        nodes={doc.nodes}
        edges={doc.edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        isValidConnection={isValidConnection}
        onNodeDragStart={checkpoint}
        onNodeDoubleClick={(_, node) => {
          // Memos and groups are edited in place.
          if (node.type === "block" || node.type === "plan" || node.type === "output") {
            void useComposerStore.getState().focusNode(node.id);
          }
        }}
        onPaneContextMenu={(e) => openMenu(e, [])}
        onNodeContextMenu={(e, node) => {
          // A node outside the selection becomes the selection.
          const ids = node.selected
            ? doc.nodes.filter((n) => n.selected).map((n) => n.id)
            : [node.id];
          if (!node.selected) useComposerStore.getState().select(ids);
          openMenu(e, ids);
        }}
        onSelectionContextMenu={(e, nodes) =>
          openMenu(
            e,
            nodes.map((n) => n.id),
          )
        }
        zoomOnDoubleClick={false}
        onBeforeDelete={async ({ nodes, edges }) => {
          // React Flow asks on every Delete key press, even with nothing selected.
          if (nodes.length === 0 && edges.length === 0) return true;
          checkpoint();
          // React Flow takes a group's nodes along; they stay unless selected too.
          const groups = nodes.filter((n) => n.type === "group").map((n) => n.id);
          if (groups.length === 0) return true;
          const kept = nodes.filter(
            (n) => n.parentId && groups.includes(n.parentId) && !n.selected,
          );
          if (kept.length === 0) return true;
          const keptIds = new Set(kept.map((n) => n.id));
          const gone = nodes.filter((n) => !keptIds.has(n.id));
          const goneIds = new Set(gone.map((n) => n.id));
          useComposerStore.getState().releaseGroups(groups, goneIds);
          return {
            nodes: gone,
            edges: edges.filter(
              (e) => e.selected || goneIds.has(e.source) || goneIds.has(e.target),
            ),
          };
        }}
        colorMode={theme}
        deleteKeyCode={["Backspace", "Delete"]}
        minZoom={0.3}
        maxZoom={1.6}
        fitView
        fitViewOptions={{ maxZoom: 1, padding: 0.2 }}
      >
        <Background variant={BackgroundVariant.Dots} gap={22} size={1.3} />
        <RatingPanel />
        <Controls showInteractive={false} position="bottom-left" />
        <MiniMap
          pannable
          zoomable
          style={{ width: 150, height: 100 }}
          position="bottom-right"
          nodeColor={(node) => {
            if (node.type === "plan") return "var(--c-kind-plan)";
            if (node.type === "memo") return "var(--c-memo-border)";
            if (node.type === "group") return "transparent";
            if (node.type !== "block") return "var(--c-border-strong)";
            const kind = blocks.get((node.data as { blockId: string }).blockId)?.kind;
            return `var(--c-kind-${kind ?? "instruction"})`;
          }}
          nodeBorderRadius={8}
        />
      </ReactFlow>
      {menu && <CanvasMenu at={menu} {...app} onClose={closeMenu} />}
    </div>
  );
}

function Toolbar({ preview, onTogglePreview }: { preview: boolean; onTogglePreview: () => void }) {
  const doc = useComposerStore((s) => s.doc);
  const rename = useComposerStore((s) => s.rename);
  const undo = useComposerStore((s) => s.undo);
  const redo = useComposerStore((s) => s.redo);
  const canUndo = useComposerStore((s) => s.past.length > 0);
  const canRedo = useComposerStore((s) => s.future.length > 0);
  const saveState = useComposerStore((s) => s.saveState);
  const error = useComposerStore((s) => s.error);
  const [name, setName] = useState(doc?.name ?? "");
  const t = useT();

  useEffect(() => {
    setName(doc?.name ?? "");
  }, [doc?.name]);

  const commitName = () => {
    const trimmed = name.trim() || t("composer.untitled");
    if (trimmed !== doc?.name) rename(trimmed);
    setName(trimmed);
  };

  return (
    <div className="flex h-11 shrink-0 items-center gap-1 border-b border-border px-3">
      <input
        aria-label={t("composer.docName")}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={commitName}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        className="min-w-0 max-w-80 flex-1 rounded-md bg-transparent px-2 py-1 font-serif text-fs-15 font-medium outline-none hover:bg-hover focus:bg-surface-raised"
      />
      <span
        className="ml-1 flex items-center gap-1 text-fs-11 text-text-subtle"
        title={error ?? undefined}
      >
        {saveState === "saving" && <Loader2 size={12} className="animate-spin" />}
        {saveState === "saved" && <Check size={12} />}
        {saveState === "error" && <CloudOff size={12} className="text-danger" />}
        {saveState === "saving"
          ? t("composer.saving")
          : saveState === "saved"
            ? t("composer.saved")
            : t("composer.saveFailed")}
      </span>
      <div className="ml-auto flex items-center gap-0.5">
        <IconButton label={t("composer.undo")} disabled={!canUndo} onClick={undo}>
          <Undo2 size={16} />
        </IconButton>
        <IconButton label={t("composer.redo")} disabled={!canRedo} onClick={redo}>
          <Redo2 size={16} />
        </IconButton>
        <IconButton
          label={preview ? t("composer.hidePreview") : t("composer.showPreview")}
          aria-pressed={preview}
          onClick={onTogglePreview}
          className={preview ? "text-text" : ""}
        >
          <PanelRight size={16} />
        </IconButton>
      </div>
    </div>
  );
}

/**
 * Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y undo and redo, Ctrl+A selects all, Ctrl+D
 * duplicates, Ctrl+G groups and Ctrl+Shift+G ungroups the selection; not
 * while typing in a field.
 */
function useCanvasShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable=true]")) return;
      if (!(e.ctrlKey || e.metaKey)) return;
      const store = useComposerStore.getState();
      const selected = (store.doc?.nodes ?? []).filter((n) => n.selected).map((n) => n.id);
      const key = e.key.toLowerCase();
      if (key === "z" && !e.shiftKey) store.undo();
      else if ((key === "z" && e.shiftKey) || key === "y") store.redo();
      else if (key === "a") store.select("all");
      else if (key === "d" && selected.length > 0) store.duplicate(selected);
      else if (key === "g" && !e.shiftKey && selected.length > 0) store.groupNodes(selected);
      else if (key === "g" && e.shiftKey && selected.length > 0) store.ungroup(selected);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
