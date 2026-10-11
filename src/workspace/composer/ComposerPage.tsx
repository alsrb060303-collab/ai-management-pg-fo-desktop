import "@xyflow/react/dist/style.css";

import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
} from "@xyflow/react";
import { Check, CloudOff, Loader2, PanelRight, Redo2, Undo2 } from "lucide-react";
import { useEffect, useState } from "react";
import { IconButton } from "../../components/ui";
import { useComposerStore } from "../../store/composer";
import { useThemeStore } from "../../store/theme";
import { nodeTypes } from "./nodes";
import { Palette } from "./Palette";
import { PreviewPanel } from "./PreviewPanel";
import { WriteDialog } from "./WriteDialog";

const PREVIEW_KEY = "composer-preview";

function readPreviewPref(): boolean {
  try {
    return localStorage.getItem(PREVIEW_KEY) !== "0";
  } catch {
    return true;
  }
}

export default function ComposerPage() {
  const init = useComposerStore((s) => s.init);
  const doc = useComposerStore((s) => s.doc);
  const [writing, setWriting] = useState(false);
  const [preview, setPreview] = useState(readPreviewPref);
  const togglePreview = () => {
    setPreview(!preview);
    try {
      localStorage.setItem(PREVIEW_KEY, preview ? "0" : "1");
    } catch {
      // Only a convenience.
    }
  };

  useEffect(() => {
    void init();
  }, [init]);

  useUndoShortcuts();

  if (!doc) {
    return <div className="p-10 text-sm text-text-muted">불러오는 중…</div>;
  }

  return (
    <ReactFlowProvider>
      <div className="flex h-full">
        <Palette />
        <div className="flex min-w-0 flex-1 flex-col">
          <Toolbar preview={preview} onTogglePreview={togglePreview} />
          <Canvas />
        </div>
        {preview && <PreviewPanel onWrite={() => setWriting(true)} />}
      </div>
      {writing && <WriteDialog doc={doc} onClose={() => setWriting(false)} />}
    </ReactFlowProvider>
  );
}

function Canvas() {
  const doc = useComposerStore((s) => s.doc);
  const onNodesChange = useComposerStore((s) => s.onNodesChange);
  const onEdgesChange = useComposerStore((s) => s.onEdgesChange);
  const onConnect = useComposerStore((s) => s.onConnect);
  const isValidConnection = useComposerStore((s) => s.isValidConnection);
  const checkpoint = useComposerStore((s) => s.checkpoint);
  const blocks = useComposerStore((s) => s.blocks);
  const theme = useThemeStore((s) => s.resolved);

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
        onBeforeDelete={async () => {
          checkpoint();
          return true;
        }}
        colorMode={theme}
        deleteKeyCode={["Backspace", "Delete"]}
        minZoom={0.3}
        maxZoom={1.6}
        fitView
        fitViewOptions={{ maxZoom: 1, padding: 0.2 }}
      >
        <Background variant={BackgroundVariant.Dots} gap={22} size={1.3} />
        <Controls showInteractive={false} position="bottom-left" />
        <MiniMap
          pannable
          zoomable
          style={{ width: 150, height: 100 }}
          position="bottom-right"
          nodeColor={(node) => {
            if (node.type !== "block") return "var(--c-border-strong)";
            const kind = blocks.get((node.data as { blockId: string }).blockId)?.kind;
            return `var(--c-kind-${kind ?? "instruction"})`;
          }}
          nodeBorderRadius={8}
        />
      </ReactFlow>
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

  useEffect(() => {
    setName(doc?.name ?? "");
  }, [doc?.name]);

  const commitName = () => {
    const trimmed = name.trim() || "새 문서";
    if (trimmed !== doc?.name) rename(trimmed);
    setName(trimmed);
  };

  return (
    <div className="flex h-11 shrink-0 items-center gap-1 border-b border-border px-3">
      <input
        aria-label="문서 이름"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={commitName}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        className="min-w-0 max-w-80 flex-1 rounded-md bg-transparent px-2 py-1 font-serif text-[15px] font-medium outline-none hover:bg-hover focus:bg-surface-raised"
      />
      <span
        className="ml-1 flex items-center gap-1 text-[11px] text-text-subtle"
        title={error ?? undefined}
      >
        {saveState === "saving" && <Loader2 size={12} className="animate-spin" />}
        {saveState === "saved" && <Check size={12} />}
        {saveState === "error" && <CloudOff size={12} className="text-danger" />}
        {saveState === "saving" ? "저장 중" : saveState === "saved" ? "저장됨" : "저장 실패"}
      </span>
      <div className="ml-auto flex items-center gap-0.5">
        <IconButton label="실행 취소 (Ctrl+Z)" disabled={!canUndo} onClick={undo}>
          <Undo2 size={16} />
        </IconButton>
        <IconButton label="다시 실행 (Ctrl+Shift+Z)" disabled={!canRedo} onClick={redo}>
          <Redo2 size={16} />
        </IconButton>
        <IconButton
          label={preview ? "미리보기 닫기" : "미리보기 열기"}
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

/** Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y, except while typing in a field. */
function useUndoShortcuts() {
  const undo = useComposerStore((s) => s.undo);
  const redo = useComposerStore((s) => s.redo);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea, [contenteditable=true]")) return;
      if (!(e.ctrlKey || e.metaKey)) return;
      const key = e.key.toLowerCase();
      if (key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
      } else if ((key === "z" && e.shiftKey) || key === "y") {
        e.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);
}
