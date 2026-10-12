import { invoke, isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";

// Mirrors src-tauri/src/mcp.rs.
export type McpScope = "project" | "local" | "user" | "other";
export type Transport = "stdio" | "http" | "sse" | "ws";
export type Health = "connected" | "failed" | "pending" | "needsAuth" | "off" | "unknown";

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
  servers: McpServer[];
  cliError: string | null;
  projectError: string | null;
  loading: boolean;
  error: string | null;
  recommended: Recommended[];
  load: (project: string | null) => Promise<void>;
  save: (project: string, draft: Draft) => Promise<void>;
  remove: (project: string, server: McpServer) => Promise<void>;
  setEnabled: (project: string, name: string, enabled: boolean) => Promise<void>;
  dismissError: () => void;
}

let seq = 0;

export const useMcpStore = create<McpState>((set, get) => ({
  project: null,
  servers: [],
  cliError: null,
  projectError: null,
  loading: false,
  error: null,
  recommended: [],

  load: async (project) => {
    if (!isTauri()) return;
    if (!get().recommended.length) {
      void invoke<Recommended[]>("mcp_recommended").then((recommended) => set({ recommended }));
    }
    if (!project) {
      set({ project: null, servers: [], cliError: null, projectError: null });
      return;
    }
    const mine = ++seq;
    set({ loading: true, ...(get().project !== project ? { project, servers: [] } : {}) });
    try {
      const listing = await invoke<{
        servers: McpServer[];
        cliError: string | null;
        projectError: string | null;
      }>("mcp_list", { project });
      if (mine === seq) set({ ...listing, loading: false });
    } catch (e) {
      if (mine === seq) set({ error: String(e), loading: false });
    }
  },

  save: async (project, draft) => {
    await invoke("mcp_save", {
      project,
      original: draft.original,
      scope: draft.scope,
      name: draft.name,
      config: draft.config,
    });
    await get().load(project);
  },

  remove: async (project, server) => {
    try {
      await invoke("mcp_remove", { project, scope: server.scope, name: server.name });
    } catch (e) {
      set({ error: String(e) });
    }
    await get().load(project);
  },

  setEnabled: async (project, name, enabled) => {
    try {
      await invoke("mcp_set_enabled", { project, name, enabled });
    } catch (e) {
      set({ error: String(e) });
    }
    await get().load(project);
  },

  dismissError: () => set({ error: null }),
}));
