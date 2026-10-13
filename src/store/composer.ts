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
import { useSyncStore } from "./sync";

// Mirrors src-tauri/src/composer.
export type BlockKind = "instruction" | "rule" | "skill" | "agent" | "hook";
export type Target = "claudeMd" | "rules" | "skills" | "agents" | "custom" | "hooks" | "plans";

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
  /** Hooks: the Claude Code event and matcher; the body is the command. */
  event?: string | null;
  matcher?: string | null;
  body: string;
  extraFiles: number;
  /** Bundled skills: files written next to SKILL.md (paths relative to it). */
  companions: string[];
  /** Bytes of text the block writes, body and companion files together. */
  size: number;
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

export type BlockData = {
  blockId: string;
  body?: string;
  /** Switched off on the canvas: left out of every output. */
  disabled?: boolean;
  /** Subagents: a model alias (`MODELS`); absent means the block's own. */
  model?: string;
  /** Hooks: event and matcher replacing the block's. */
  event?: string;
  matcher?: string;
};
export type OutputData = {
  target: Target;
  scope: Mode;
  order: string[];
  /** Custom outputs: the file's path and the user's own text. */
  path?: string;
  text?: string;
};
/** Plan goals: `y` done, `p` in progress, `n` not yet. */
export type PlanStatus = "y" | "p" | "n";
export type PlanAction = "text" | "agent" | "skill" | "ask";
/** A row under a main goal (mirrors `plans::PlanItem`). */
export interface PlanItem {
  id: string;
  kind: "sub" | "detail" | "step";
  text: string;
  note?: string;
  status?: PlanStatus;
  /** Steps: what happens at this point. */
  action?: PlanAction;
  /** Agent and skill steps: the catalog block. */
  block?: string;
}
/** A main goal and everything under it (a plan node). */
export type PlanData = {
  title: string;
  note?: string;
  status?: PlanStatus;
  items: PlanItem[];
  disabled?: boolean;
};
export type BlockNode = Node<BlockData, "block">;
export type OutputNode = Node<OutputData, "output">;
export type PlanNode = Node<PlanData, "plan">;
export type ComposerNode = BlockNode | OutputNode | PlanNode;

/**
 * A detail goal needs a sub-goal above it: one that ends up first becomes a
 * sub-goal (compile.rs reads it the same way).
 */
export function fixPlan(items: PlanItem[]): PlanItem[] {
  let sub = false;
  return items.map((item) => {
    if (item.kind === "sub") sub = true;
    if (item.kind === "detail" && !sub) {
      sub = true;
      return { ...item, kind: "sub" };
    }
    return item;
  });
}

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
  custom: ["instruction", "rule"],
  hooks: ["hook"],
  // Plan nodes (main goals), not catalog blocks.
  plans: [],
};

/** Subagent model aliases (`MODEL_ALIASES` in compile.rs) and their current model. */
export const MODELS: { alias: string; label: string }[] = [
  { alias: "fable", label: "Fable 5.1" },
  { alias: "opus", label: "Opus 5.5" },
  { alias: "sonnet", label: "Sonnet 5.5" },
  { alias: "haiku", label: "Haiku 5.5" },
];

/** Hook events offered first; the rest of `HOOK_EVENTS` in compile.rs follow. */
export const HOOK_EVENTS = [
  "PreToolUse",
  "PostToolUse",
  "UserPromptSubmit",
  "Stop",
  "SubagentStop",
  "SessionStart",
  "SessionEnd",
  "Notification",
  "PreCompact",
  "PostToolUseFailure",
  "PermissionRequest",
  "SubagentStart",
  "PostCompact",
  "FileChanged",
  "CwdChanged",
  "InstructionsLoaded",
  "ConfigChange",
  "TaskCreated",
  "TaskCompleted",
];

/** Events whose matcher Claude Code ignores (`EVENTS_WITHOUT_MATCHER`). */
export const EVENTS_WITHOUT_MATCHER = new Set([
  "UserPromptSubmit",
  "PostToolBatch",
  "Stop",
  "TeammateIdle",
  "TaskCreated",
  "TaskCompleted",
  "WorktreeCreate",
  "WorktreeRemove",
  "MessageDisplay",
  "CwdChanged",
]);

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
  /**
   * After sync changed documents on disk: reloads the list, and the open
   * document unless it has an edit waiting to be saved.
   */
  reloadFromDisk: (ids: string[]) => Promise<void>;
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
  /** Merges into a block node's data; `undefined` values remove the key. */
  setBlockData: (nodeId: string, patch: Partial<Omit<BlockData, "blockId">>) => void;
  setOutputData: (nodeId: string, patch: Partial<Pick<OutputData, "path" | "text">>) => void;
  setScope: (nodeId: string, scope: Mode) => void;
  /** A new main goal; returns its node id. */
  addPlan: (position: XYPosition, title?: string) => string;
  setPlan: (nodeId: string, patch: Partial<Omit<PlanData, "items">>) => void;
  /** Inserts a row at `index` (the end when omitted); returns its id. */
  addPlanItem: (
    nodeId: string,
    item: Pick<PlanItem, "kind"> & Partial<PlanItem>,
    index?: number,
  ) => string;
  setPlanItem: (nodeId: string, itemId: string, patch: Partial<Omit<PlanItem, "id">>) => void;
  movePlanItem: (nodeId: string, from: number, to: number) => void;
  removePlanItem: (nodeId: string, itemId: string) => void;
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
  hook: "hooks",
};

/** JSON with sorted keys, to compare documents however they were built. */
function canonical(doc: ComposerDoc): string {
  const sort = (v: unknown): unknown =>
    Array.isArray(v)
      ? v.map(sort)
      : v && typeof v === "object"
        ? Object.fromEntries(
            Object.keys(v)
              .sort()
              .map((k) => [k, sort((v as Record<string, unknown>)[k])]),
          )
        : v;
  return JSON.stringify(sort(persistable(doc)));
}

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

const edgeColor = (kind: BlockKind | "plan" | undefined) =>
  `var(--c-kind-${kind ?? "instruction"})`;

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

  /** The open document as it was last read from or written to disk. */
  let onDisk = "";

  const save = async (doc: ComposerDoc) => {
    if (!isTauri()) return;
    try {
      await invoke("composer_doc_save", { doc: persistable(doc) });
      if (get().doc?.id === doc.id) onDisk = canonical(doc);
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

  const kindOf = (doc: ComposerDoc, nodeId: string): BlockKind | "plan" | undefined => {
    const node = doc.nodes.find((n) => n.id === nodeId);
    if (node?.type === "plan") return "plan";
    return node?.type === "block" ? get().blocks.get(node.data.blockId)?.kind : undefined;
  };

  /** Applies `f` to a plan node's data. */
  const updatePlan = (nodeId: string, f: (data: PlanData) => PlanData) =>
    update((doc) => ({
      ...doc,
      nodes: doc.nodes.map((n) =>
        n.id === nodeId && n.type === "plan" ? { ...n, data: f(n.data) } : n,
      ),
    }));

  const open = (doc: ComposerDoc) => {
    onDisk = canonical(doc);
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

    reloadFromDisk: async (ids) => {
      if (!isTauri()) return;
      await get().loadDocs();
      const { doc, docs, saveState } = get();
      if (!doc || !ids.includes(doc.id)) return;
      // Edits not on disk yet: they must not be lost, nor the other device's.
      const pending = saveTimer !== undefined || saveState === "saving";
      if (!docs.some((d) => d.id === doc.id)) {
        // Deleted on another device; unsaved edits here bring it back.
        if (pending) return;
        const next = docs[0];
        if (next) {
          set({ doc: null });
          await get().openDoc(next.id);
        } else {
          await get().newDoc();
        }
        return;
      }
      try {
        const loaded = await invoke<ComposerDoc>("composer_doc_load", { id: doc.id });
        const fresh = { ...blankDoc(), ...loaded, id: doc.id };
        if (canonical(fresh) === onDisk) return;
        if (pending || saveTimer !== undefined) {
          // Keep editing ours; theirs is kept next to it as a copy.
          const name = `${fresh.name} (${t("sync.otherDevice")})`;
          const copy = { ...fresh, id: newId("d"), name, updatedAt: Date.now() };
          await invoke("composer_doc_save", { doc: persistable(copy) });
          onDisk = canonical(fresh);
          useSyncStore
            .getState()
            .addConflicts([{ kind: "doc", name: fresh.name, copy: name, revived: false }]);
          await get().loadDocs();
          return;
        }
        open(fresh);
      } catch (e) {
        set({ error: String(e) });
      }
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
      if (target?.type !== "output") return false;
      if (source?.type === "plan") {
        if (target.data.target !== "plans") return false;
      } else {
        if (source?.type !== "block") return false;
        const kind = blocks.get(source.data.blockId)?.kind;
        if (!kind || !ACCEPTS[target.data.target].includes(kind)) return false;
      }
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
      if (!doc || !node || node.type === "output" || !isTauri()) return;
      // Already part of an output's file: the preview shows that one.
      if (compiled?.files.some((f) => f.blocks.includes(node.id))) return;
      const kind = node.type === "plan" ? "plan" : blocks.get(node.data.blockId)?.kind;
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
            data: {
              target: kind === "plan" ? "plans" : PREVIEW_TARGET[kind],
              scope: "team",
              order: [node.id],
            },
          },
        ],
        edges: [{ id: "preview-edge", source: node.id, target: out }],
      };
      try {
        const result = await invoke<Compiled>("composer_compile", { doc: temp });
        // A plan's preview also points CLAUDE.md to it; show PLANS.md.
        const file = result.files.find((f) => f.node === out) ?? null;
        if (get().focus === nodeId) set({ standalone: file });
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

    setBlockData: (nodeId, patch) =>
      update((doc) => ({
        ...doc,
        nodes: doc.nodes.map((n) => {
          if (n.id !== nodeId || n.type !== "block") return n;
          const data: BlockData = { ...n.data, ...patch };
          for (const key of Object.keys(patch) as (keyof typeof patch)[]) {
            if (patch[key] === undefined) delete data[key];
          }
          return { ...n, data };
        }),
      })),

    setOutputData: (nodeId, patch) =>
      update((doc) => ({
        ...doc,
        nodes: doc.nodes.map((n) =>
          n.id === nodeId && n.type === "output" ? { ...n, data: { ...n.data, ...patch } } : n,
        ),
      })),

    setScope: (nodeId, scope) =>
      update((doc) => ({
        ...doc,
        nodes: doc.nodes.map((n) =>
          n.id === nodeId && n.type === "output" ? { ...n, data: { ...n.data, scope } } : n,
        ),
      })),

    addPlan: (position, title) => {
      const id = newId("p");
      update((doc) => ({
        ...doc,
        nodes: [
          ...doc.nodes.map((n) => ({ ...n, selected: false })),
          {
            id,
            type: "plan",
            position,
            data: { title: title ?? "", status: "n", items: [] },
            selected: true,
          },
        ],
      }));
      return id;
    },

    setPlan: (nodeId, patch) => updatePlan(nodeId, (data) => ({ ...data, ...patch })),

    addPlanItem: (nodeId, item, index) => {
      const id = newId("i");
      const row: PlanItem = {
        text: "",
        ...(item.kind === "step" ? { action: "text" as const } : { status: "n" as const }),
        ...item,
        id,
      };
      updatePlan(nodeId, (data) => {
        const items = [...data.items];
        items.splice(index ?? items.length, 0, row);
        return { ...data, items: fixPlan(items) };
      });
      return id;
    },

    setPlanItem: (nodeId, itemId, patch) =>
      updatePlan(nodeId, (data) => ({
        ...data,
        items: fixPlan(data.items.map((i) => (i.id === itemId ? { ...i, ...patch } : i))),
      })),

    movePlanItem: (nodeId, from, to) => {
      if (from === to) return;
      updatePlan(nodeId, (data) => {
        const items = [...data.items];
        if (!items[from]) return data;
        // A sub-goal takes its detail goals and steps along.
        let size = 1;
        if (items[from].kind === "sub") {
          while (from + size < items.length && items[from + size].kind !== "sub") size++;
        }
        if (to >= from && to < from + size) return data;
        const moved = items.splice(from, size);
        items.splice(to > from ? to - size + 1 : to, 0, ...moved);
        return { ...data, items: fixPlan(items) };
      });
    },

    removePlanItem: (nodeId, itemId) =>
      updatePlan(nodeId, (data) => ({
        ...data,
        items: fixPlan(data.items.filter((i) => i.id !== itemId)),
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
