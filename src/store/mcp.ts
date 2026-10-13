import { invoke, isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";
import type { ToolId } from "../tools";
import { useProjectStore, whenToolReady } from "./project";

// Mirrors src-tauri/src/mcp.rs.
export type McpScope = "project" | "local" | "user" | "other";
export type Transport = "stdio" | "http" | "sse" | "ws";
export type Health =
  | "connected"
  | "failed"
  | "pending"
  | "needsAuth"
  | "off"
  | "unknown"
  /** The tool has no connection check (Codex). */
  | "noCheck";
/** Codex: whether the project's `.codex/config.toml` is read. */
export type Trust = "trusted" | "untrusted" | "unknown";

export interface McpConfig {
  transport: Transport;
  command: string;
  args: string[];
  env: Record<string, string>;
  url: string;
  headers: Record<string, string>;
}

export interface McpServer {
  name: string;
  scope: McpScope;
  config: McpConfig | null;
  target: string;
  health: Health;
  status: string;
  enabled: boolean | null;
  argsApprox: boolean;
}

export interface Parsed {
  name: string;
  scope: McpScope | null;
  config: McpConfig;
}

export interface Recommended {
  id: string;
  name: string;
  repo: string;
  license: string;
  needs: string;
  config: McpConfig;
}

/** What the edit form starts from; `original` is set when editing. */
export interface Draft {
  original: { scope: McpScope; name: string; config: McpConfig | null } | null;
  scope: McpScope;
  name: string;
  config: McpConfig;
  argsApprox?: boolean;
}

export const emptyConfig = (): McpConfig => ({
  transport: "stdio",
  command: "",
  args: [],
  env: {},
  url: "",
  headers: {},
});

/** A value that reads like a real secret rather than a `${VAR}` reference. */
export function looksSecret(key: string, value: string): boolean {
  if (!value.trim() || value.includes("${")) return false;
  const v = value.replace(/^(bearer|basic|token)\s+/i, "").trim();
  return /(key|token|secret|pass|auth|bearer|credential|pat)/i.test(key) && v.length >= 8;
}

/**
 * `${NAME}` to put in place of a secret; the user sets NAME themselves. An
 * env var keeps its own name; a header gets one from the server's name.
 */
export function variableFor(
  kind: "env" | "header",
  server: string,
  key: string,
  value: string,
): string {
  const upper = (s: string) =>
    s
      .replace(/[^A-Za-z0-9]+/g, "_")
      .replace(/^_+|_+$/g, "")
      .toUpperCase();
  if (kind === "env") return `\${${upper(key)}}`;
  const name = /^authorization$/i.test(key) ? `${upper(server)}_TOKEN` : upper(`${server}_${key}`);
  const scheme = value.match(/^(bearer|basic|token)\s+/i)?.[0] ?? "";
  return `${scheme}\${${name}}`;
}

interface McpState {
  project: string | null;
  /** The tool whose servers are shown (the project's tool). */
  tool: ToolId;
  servers: McpServer[];
  cliError: string | null;
  projectError: string | null;
  /** Codex: the user config.toml can't be read. */
  userError: string | null;
  /** Codex only. */
  trust: Trust | null;
  loading: boolean;
  error: string | null;
  recommended: Recommended[];
  /** `tool` defaults to the current project's tool. */
  load: (project: string | null, tool?: ToolId) => Promise<void>;
  save: (project: string, draft: Draft) => Promise<void>;
  remove: (project: string, server: McpServer) => Promise<void>;
  setEnabled: (project: string, server: McpServer, enabled: boolean) => Promise<void>;
  dismissError: () => void;
}

let seq = 0;

export const useMcpStore = create<McpState>((set, get) => ({
  project: null,
  tool: useProjectStore.getState().tool,
  servers: [],
  cliError: null,
  projectError: null,
  userError: null,
  trust: null,
  loading: false,
  error: null,
  recommended: [],

  load: async (project, toolArg) => {
    const tool = toolArg ?? (await whenToolReady());
    if (!isTauri()) return;
    if (!get().recommended.length) {
      void invoke<Recommended[]>("mcp_recommended").then((recommended) => set({ recommended }));
    }
    if (!project) {
      set({
        project: null,
        tool,
        servers: [],
        cliError: null,
        projectError: null,
        userError: null,
        trust: null,
      });
      return;
    }
    const mine = ++seq;
    const changed = get().project !== project || get().tool !== tool;
    set({ loading: true, ...(changed ? { project, tool, servers: [], trust: null } : {}) });
    try {
      const listing = await invoke<{
        servers: McpServer[];
        cliError: string | null;
        projectError: string | null;
        userError: string | null;
        trust: Trust | null;
      }>("mcp_list", { project, tool });
      if (mine === seq) set({ ...listing, loading: false });
    } catch (e) {
      if (mine === seq) set({ error: String(e), loading: false });
    }
  },

  save: async (project, draft) => {
    await invoke("mcp_save", {
      project,
      tool: get().tool,
      original: draft.original,
      scope: draft.scope,
      name: draft.name,
      config: draft.config,
    });
    await get().load(project, get().tool);
  },

  remove: async (project, server) => {
    const { tool } = get();
    try {
      await invoke("mcp_remove", { project, tool, scope: server.scope, name: server.name });
    } catch (e) {
      set({ error: String(e) });
    }
    await get().load(project, tool);
  },

  setEnabled: async (project, server, enabled) => {
    const { tool } = get();
    try {
      await invoke("mcp_set_enabled", {
        project,
        tool,
        scope: server.scope,
        name: server.name,
        enabled,
      });
    } catch (e) {
      set({ error: String(e) });
    }
    await get().load(project, tool);
  },

  dismissError: () => set({ error: null }),
}));
