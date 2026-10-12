import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  type XYPosition,
} from "@xyflow/react";
import { create } from "zustand";
import { onLocaleApplied, t, useLocaleStore } from "../i18n";
import type { Mode } from "./library";

// Mirrors src-tauri/src/composer.
export type BlockKind = "instruction" | "rule" | "skill" | "agent";
export type Target = "claudeMd" | "rules" | "skills" | "agents";

export interface Block {
  id: string;
  kind: BlockKind;
  title: string;
  summary: string;
  category: string;
  /** A few sentences on what the block makes Claude do (shown when opened). */
  detail?: string | null;
  source: "builtin" | "library" | "external";
  name: string;
  description: string | null;
  paths: string[];
  tools: string | null;
  model: string | null;
  body: string;
  extraFiles: number;
  /** English title, summary and category, when the block has them. */
  en?: { title: string; summary: string; category: string; detail?: string | null };
  /** Third-party blocks: where they come from (kept in written files). */
  origin: {
    repo: string;
    commit: string;
    path: string;
    license: string;
    copyright: string;
  } | null;
}

export const originUrl = (o: NonNullable<Block["origin"]>) =>
  `https://github.com/${o.repo}/blob/${o.commit}/${o.path}`;

export interface OutFile {
  path: string;
  content: string;
  section: boolean;
  personal: boolean;
  target: Target;
  node: string;
  blocks: string[];
  lines: number;
}

export interface Compiled {
  files: OutFile[];
  warnings: { node: string | null; message: string }[];
}

export interface DocMeta {
  id: string;
  name: string;
  updatedAt: number;
}

export type BlockData = { blockId: string; body?: string };
export type OutputData = { target: Target; scope: Mode; order: string[] };
export type BlockNode = Node<BlockData, "block">;
export type OutputNode = Node<OutputData, "output">;
export type ComposerNode = BlockNode | OutputNode;

export interface ComposerDoc {
  version: 1;
  id: string;
  name: string;
  updatedAt: number;
  nodes: ComposerNode[];
  edges: Edge[];
}

/** Which block kinds each output accepts (same rule as `Target::accepts`). */
export const ACCEPTS: Record<Target, BlockKind[]> = {
  claudeMd: ["instruction"],
  rules: ["rule", "instruction"],
  skills: ["skill"],
  agents: ["agent"],
};

export const CLAUDE_MD_LINE_BUDGET = 200;
const HISTORY_LIMIT = 100;

type Snapshot = { nodes: ComposerNode[]; edges: Edge[] };

interface ComposerState {
  catalog: Block[];
  blocks: Map<string, Block>;
  docs: DocMeta[];
  doc: ComposerDoc | null;
  compiled: Compiled | null;
  saveState: "saved" | "saving" | "error";
  error: string | null;
  past: Snapshot[];
  future: Snapshot[];

  init: () => Promise<void>;
  loadCatalog: () => Promise<void>;
  /** Updates the bundled third-party blocks from upstream; returns a summary. */
  refreshExternal: () => Promise<string>;
  loadDocs: () => Promise<void>;
  newDoc: () => Promise<void>;
  openDoc: (id: string) => Promise<void>;
  deleteDoc: (id: string) => Promise<void>;
  /** Saves pending edits right away (before switching documents or windows). */
  flushSave: () => Promise<void>;
  rename: (name: string) => void;

  onNodesChange: (changes: NodeChange<ComposerNode>[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  isValidConnection: (connection: Connection | Edge) => boolean;
  /** Call before a drag so the move can be undone in one step. */
  checkpoint: () => void;

  addBlock: (blockId: string, position: XYPosition) => void;
  addOutput: (target: Target, position: XYPosition) => void;
  setBody: (nodeId: string, body: string | undefined) => void;
  setScope: (nodeId: string, scope: Mode) => void;
  reorder: (nodeId: string, from: number, to: number) => void;
  undo: () => void;
  redo: () => void;

  /** The node whose file the preview shows (double-clicked on the canvas). */
  focus: string | null;
  /**
   * For a focused block no output uses yet: the file it would be written as,
   * compiled on its own.
   */
  standalone: OutFile | null;
  focusNode: (nodeId: string | null) => Promise<void>;
}

/** The output a block of each kind goes to when previewed on its own. */
const PREVIEW_TARGET: Record<BlockKind, Target> = {
  instruction: "claudeMd",
  rule: "rules",
  skill: "skills",
  agent: "agents",
};

function newId(prefix: string): string {
  return `${prefix}${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
}

/** Block node ids connected to an output node, in document order. */
export function inputsOf(doc: ComposerDoc, outputId: string): string[] {
  const output = doc.nodes.find((n) => n.id === outputId);
  const connected = doc.edges.filter((e) => e.target === outputId).map((e) => e.source);
  const order = output?.type === "output" ? output.data.order : [];
  const result: string[] = [];
  for (const id of [...order, ...connected]) {
    if (connected.includes(id) && !result.includes(id)) result.push(id);
  }
  return result;
}

function blankDoc(): ComposerDoc {
  return {
    version: 1,
    id: newId("d"),
    name: t("composer.untitled"),
    updatedAt: Date.now(),
    nodes: [
      {
        id: newId("o"),
        type: "output",
        position: { x: 640, y: 160 },
        data: { target: "claudeMd", scope: "team", order: [] },
      },
    ],
    edges: [],
  };
}

/** Drops transient React Flow state before saving. */
function persistable(doc: ComposerDoc): ComposerDoc {
  return {
    ...doc,
    nodes: doc.nodes.map(({ selected: _s, dragging: _d, ...node }) => node) as ComposerNode[],
    edges: doc.edges.map(({ selected: _s, ...edge }) => edge),
  };
}

const edgeColor = (kind: BlockKind | undefined) => `var(--c-kind-${kind ?? "instruction"})`;

/** The catalog as received; `catalog` in the store is it in the UI language. */
let rawCatalog: Block[] = [];

function localized(raw: Block[]): Pick<ComposerState, "catalog" | "blocks"> {
  const english = useLocaleStore.getState().locale === "en";
  const catalog = raw.map((b) => (english && b.en ? { ...b, ...b.en } : b));
  return { catalog, blocks: new Map(catalog.map((b) => [b.id, b])) };
}

export const useComposerStore = create<ComposerState>((set, get) => {
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let compileTimer: ReturnType<typeof setTimeout> | undefined;
  let compileSeq = 0;

  const scheduleCompile = () => {
    clearTimeout(compileTimer);
    compileTimer = setTimeout(async () => {
      const { doc } = get();
      if (!doc || !isTauri()) return;
      const seq = ++compileSeq;
      try {
        const compiled = await invoke<Compiled>("composer_compile", { doc: persistable(doc) });
        if (seq === compileSeq) set({ compiled });
      } catch (e) {
        if (seq === compileSeq) set({ error: String(e) });
      }
    }, 120);
  };

  const save = async (doc: ComposerDoc) => {
    if (!isTauri()) return;
    try {
      await invoke("composer_doc_save", { doc: persistable(doc) });
      if (get().doc?.id === doc.id) set({ saveState: "saved" });
      void get().loadDocs();
    } catch (e) {
      set({ saveState: "error", error: String(e) });
    }
  };

  const scheduleSave = () => {
    set({ saveState: "saving" });
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = undefined;
      const { doc } = get();
      if (doc) void save(doc);
    }, 600);
  };

  /** Drops a pending save and compile, e.g. for a document being deleted. */
  const cancelPending = () => {
    clearTimeout(saveTimer);
    clearTimeout(compileTimer);
    saveTimer = undefined;
  };

  /**
   * Applies a change to the open document, then saves it. `record` makes it
   * undoable; `compile` refreshes the preview (moves do not change output).
   */
  const update = (f: (doc: ComposerDoc) => ComposerDoc, record = true, compile = true) => {
    const { doc } = get();
    if (!doc) return;
    if (record) pushHistory();
    set({ doc: { ...f(doc), updatedAt: Date.now() } });
    if (compile) scheduleCompile();
    scheduleSave();
  };

  const pushHistory = () => {
    const { doc, past } = get();
    if (!doc) return;
    set({
      past: [...past, { nodes: doc.nodes, edges: doc.edges }].slice(-HISTORY_LIMIT),
      future: [],
    });
  };

  const kindOf = (doc: ComposerDoc, nodeId: string): BlockKind | undefined => {
    const node = doc.nodes.find((n) => n.id === nodeId);
    return node?.type === "block" ? get().blocks.get(node.data.blockId)?.kind : undefined;
  };

  const open = (doc: ComposerDoc) => {
    set({
      doc,
      past: [],
      future: [],
      compiled: null,
      saveState: "saved",
      error: null,
      focus: null,
      standalone: null,
    });
    scheduleCompile();
  };

  // Compile warnings come from the backend: redo them in the new language.
  onLocaleApplied(() => scheduleCompile());
  return {
    catalog: [],
    blocks: new Map(),
    docs: [],
    doc: null,
    compiled: null,
    saveState: "saved",
    error: null,
    past: [],
    future: [],

    init: async () => {
      await Promise.all([get().loadCatalog(), get().loadDocs()]);
      if (get().doc) return;
      const latest = get().docs[0];
      if (latest) await get().openDoc(latest.id);
      else get().newDoc();
    },

    loadCatalog: async () => {
      if (!isTauri()) return;
      try {
        rawCatalog = await invoke<Block[]>("composer_catalog");
        set(localized(rawCatalog));
        scheduleCompile();
      } catch (e) {
        set({ error: String(e) });
      }
    },

    refreshExternal: async () => {
      if (!isTauri()) return "";
      const r = await invoke<{ updated: string[]; unchanged: number; failed: [string, string][] }>(
        "composer_refresh_external",
      );
      await get().loadCatalog();
      const parts = [
        r.updated.length > 0
          ? t("composer.palette.updated", { n: r.updated.length })
          : t("composer.palette.upToDate"),
        r.failed.length > 0
          ? t("composer.palette.failed", { n: r.failed.length, reason: r.failed[0][1] })
          : "",
      ];
      return parts.filter(Boolean).join(" · ");
    },

    loadDocs: async () => {
      if (!isTauri()) return;
      try {
        set({ docs: await invoke<DocMeta[]>("composer_docs") });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    flushSave: async () => {
      if (saveTimer === undefined) return;
      cancelPending();
      const { doc } = get();
      if (doc) await save(doc);
    },

    newDoc: async () => {
      await get().flushSave();
      open(blankDoc());
      scheduleSave();
    },

    openDoc: async (id) => {
      if (!isTauri() || get().doc?.id === id) return;
      await get().flushSave();
      try {
        const doc = await invoke<ComposerDoc>("composer_doc_load", { id });
        open({ ...blankDoc(), ...doc, id });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    deleteDoc: async (id) => {
      if (!isTauri()) return;
      const isOpen = get().doc?.id === id;
      // A pending save would bring the deleted document back.
      if (isOpen) cancelPending();
      else await get().flushSave();
      try {
        await invoke("composer_doc_delete", { id });
        await get().loadDocs();
        if (isOpen) {
          set({ doc: null, compiled: null, past: [], future: [] });
          const next = get().docs[0];
          if (next) await get().openDoc(next.id);
          else await get().newDoc();
        }
      } catch (e) {
        set({ error: String(e) });
      }
    },

    rename: (name) => update((doc) => ({ ...doc, name }), false),

    onNodesChange: (changes) => {
      // History: deletions are recorded once in onBeforeDelete, moves at drag
      // start, so one undo restores a node together with its edges.
      const meaningful = changes.some((c) => c.type !== "select" && c.type !== "dimensions");
      const moveOnly = changes.every(
        (c) => c.type !== "remove" && c.type !== "add" && c.type !== "replace",
      );
      if (!meaningful) {
        const { doc } = get();
        if (doc) set({ doc: { ...doc, nodes: applyNodeChanges(changes, doc.nodes) } });
        return;
      }
      update(
        (doc) => {
          const nodes = applyNodeChanges(changes, doc.nodes);
          const ids = new Set(nodes.map((n) => n.id));
          // Forget order entries and edges of removed nodes.
          return {
            ...doc,
            nodes: nodes.map((n) =>
              n.type === "output"
                ? { ...n, data: { ...n.data, order: n.data.order.filter((id) => ids.has(id)) } }
                : n,
            ),
            edges: doc.edges.filter((e) => ids.has(e.source) && ids.has(e.target)),
          };
        },
        false,
        !moveOnly,
      );
    },

    onEdgesChange: (changes) => {
      const removes = changes.some((c) => c.type === "remove");
      if (changes.every((c) => c.type === "select")) {
        const { doc } = get();
        if (doc) set({ doc: { ...doc, edges: applyEdgeChanges(changes, doc.edges) } });
        return;
      }
      update((doc) => {
        const edges = applyEdgeChanges(changes, doc.edges);
        if (!removes) return { ...doc, edges };
        return {
          ...doc,
          edges,
          nodes: doc.nodes.map((n) => {
            if (n.type !== "output") return n;
            const connected = new Set(edges.filter((e) => e.target === n.id).map((e) => e.source));
            return {
              ...n,
              data: { ...n.data, order: n.data.order.filter((id) => connected.has(id)) },
            };
          }),
        };
      }, false);
    },

    isValidConnection: (connection) => {
      const { doc, blocks } = get();
      if (!doc) return false;
      const source = doc.nodes.find((n) => n.id === connection.source);
      const target = doc.nodes.find((n) => n.id === connection.target);
      if (source?.type !== "block" || target?.type !== "output") return false;
      const kind = blocks.get(source.data.blockId)?.kind;
      if (!kind || !ACCEPTS[target.data.target].includes(kind)) return false;
      return !doc.edges.some((e) => e.source === source.id && e.target === target.id);
    },

    onConnect: (connection) => {
      if (!get().isValidConnection(connection)) return;
      update((doc) => {
        const color = edgeColor(kindOf(doc, connection.source));
        const edges = addEdge(
          { ...connection, style: { stroke: color, strokeWidth: 2 } },
          doc.edges,
        );
        return {
          ...doc,
          edges,
          nodes: doc.nodes.map((n) =>
            n.id === connection.target && n.type === "output"
              ? { ...n, data: { ...n.data, order: [...n.data.order, connection.source] } }
              : n,
          ),
        };
      });
    },

    checkpoint: pushHistory,

    addBlock: (blockId, position) =>
      update((doc) => ({
        ...doc,
        nodes: [
          ...doc.nodes.map((n) => ({ ...n, selected: false })),
          { id: newId("b"), type: "block", position, data: { blockId }, selected: true },
        ],
      })),

    focus: null,
    standalone: null,

    focusNode: async (nodeId) => {
      set({ focus: nodeId, standalone: null });
      const { doc, compiled, blocks } = get();
      const node = doc?.nodes.find((n) => n.id === nodeId);
      if (!doc || !node || node.type !== "block" || !isTauri()) return;
      // Already part of an output's file: the preview shows that one.
      if (compiled?.files.some((f) => f.blocks.includes(node.id))) return;
      const kind = blocks.get(node.data.blockId)?.kind;
      if (!kind) return;
      const { selected: _s, dragging: _d, ...block } = node;
      const out = "preview-output";
      const temp: ComposerDoc = {
        ...doc,
        nodes: [
          block as ComposerNode,
          {
            id: out,
            type: "output",
            position: { x: 0, y: 0 },
            data: { target: PREVIEW_TARGET[kind], scope: "team", order: [node.id] },
          },
        ],
        edges: [{ id: "preview-edge", source: node.id, target: out }],
      };
      try {
        const result = await invoke<Compiled>("composer_compile", { doc: temp });
        if (get().focus === nodeId) set({ standalone: result.files[0] ?? null });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    addOutput: (target, position) =>
      update((doc) => ({
        ...doc,
        nodes: [
          ...doc.nodes,
          {
            id: newId("o"),
            type: "output",
            position,
            data: { target, scope: "team", order: [] },
          },
        ],
      })),

    setBody: (nodeId, body) =>
      update((doc) => ({
        ...doc,
        nodes: doc.nodes.map((n) =>
          n.id === nodeId && n.type === "block" ? { ...n, data: { ...n.data, body } } : n,
        ),
      })),

    setScope: (nodeId, scope) =>
      update((doc) => ({
        ...doc,
        nodes: doc.nodes.map((n) =>
          n.id === nodeId && n.type === "output" ? { ...n, data: { ...n.data, scope } } : n,
        ),
      })),

    reorder: (nodeId, from, to) => {
      const { doc } = get();
      if (!doc || from === to) return;
      const order = inputsOf(doc, nodeId);
      const [moved] = order.splice(from, 1);
      if (moved === undefined) return;
      order.splice(to, 0, moved);
      update((d) => ({
        ...d,
        nodes: d.nodes.map((n) =>
          n.id === nodeId && n.type === "output" ? { ...n, data: { ...n.data, order } } : n,
        ),
      }));
    },

    undo: () => {
      const { doc, past, future } = get();
      const previous = past[past.length - 1];
      if (!doc || !previous) return;
      set({
        past: past.slice(0, -1),
        future: [{ nodes: doc.nodes, edges: doc.edges }, ...future],
      });
      update((d) => ({ ...d, ...previous }), false);
    },

    redo: () => {
      const { doc, past, future } = get();
      const next = future[0];
      if (!doc || !next) return;
      set({ past: [...past, { nodes: doc.nodes, edges: doc.edges }], future: future.slice(1) });
      update((d) => ({ ...d, ...next }), false);
    },
  };
});

// Block titles and summaries follow the language.
useLocaleStore.subscribe((state, prev) => {
  if (state.locale !== prev.locale) useComposerStore.setState(localized(rawCatalog));
});
