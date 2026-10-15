import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

// Mirrors src-tauri/src/automation/.
export type Schedule =
  | { kind: "daily"; time: string; days: number[] }
  | { kind: "interval"; minutes: number };

export interface AgentDef {
  description: string;
  prompt: string;
  model?: string;
  tools?: string[];
}

export interface Automation {
  id: string;
  name: string;
  enabled: boolean;
  schedule: Schedule;
  project: string;
  model: string;
  prompt: string;
  instructions: string;
  agents: Record<string, AgentDef>;
  allowedTools: string[];
  email: string;
  mailTool: string;
  docId?: string;
  createdAt: number;
  updatedAt: number;
  lastFiredAt?: number;
}

export type RunStatus = "running" | "success" | "failed";
export type Trigger = "schedule" | "manual" | "mcp";

export interface Run {
  id: string;
  automationId: string;
  name: string;
  trigger: Trigger;
  startedAt: number;
  endedAt?: number;
  status: RunStatus;
  result: string;
  error?: string;
  costUsd?: number;
}

export type Item = Automation & { nextRunAt: number | null; running: boolean };

interface Overview {
  claudeFound: boolean;
  automations: Item[];
  runs: Run[];
}

export const DEFAULT_MAIL_TOOL = "mcp__claude_ai_Gmail";

/** A new automation's settings: every morning at ten, web tools allowed. */
export function blankAutomation(): Automation {
  return {
    id: "",
    name: "",
    enabled: true,
    schedule: { kind: "daily", time: "10:00", days: [] },
    project: "",
    model: "",
    prompt: "",
    instructions: "",
    agents: {},
    allowedTools: ["WebSearch", "WebFetch"],
    email: "",
    mailTool: DEFAULT_MAIL_TOOL,
    createdAt: 0,
    updatedAt: 0,
  };
}

interface AutomationState {
  loaded: boolean;
  claudeFound: boolean;
  items: Item[];
  /** Newest first. */
  runs: Run[];
  load: () => Promise<void>;
  save: (automation: Automation) => Promise<Automation>;
  remove: (id: string) => Promise<void>;
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  run: (id: string) => Promise<void>;
}

let listening = false;

export const useAutomationStore = create<AutomationState>((set, get) => ({
  loaded: false,
  claudeFound: true,
  items: [],
  runs: [],

  load: async () => {
    if (!isTauri()) {
      set({ loaded: true });
      return;
    }
    if (!listening) {
      listening = true;
      void listen("automations-changed", () => void get().load());
    }
    const o = await invoke<Overview>("automations_overview");
    set({ loaded: true, claudeFound: o.claudeFound, items: o.automations, runs: o.runs });
  },

  save: async (automation) => {
    const saved = await invoke<Automation>("automation_save", { automation });
    await get().load();
    return saved;
  },

  remove: async (id) => {
    await invoke("automation_delete", { id });
    await get().load();
  },

  setEnabled: async (id, enabled) => {
    await invoke("automation_set_enabled", { id, enabled });
    await get().load();
  },

  run: async (id) => {
    await invoke("automation_run", { id });
    await get().load();
  },
}));
