/**
 * Supported AI coding tools. Mirrors `ToolId` and its layout methods in
 * src-tauri/src/tools/mod.rs; add a tool there and here together.
 */
export type ToolId = "claude" | "codex";
/** A project's tool choice: detected automatically, or picked by the user. */
export type ToolPref = "auto" | ToolId;

export interface ToolInfo {
  id: ToolId;
  /** Product name; not translated. */
  name: string;
  /** Always-loaded instructions file (team, personal). */
  instructions: { team: string; personal: string };
  skillsDir: string;
  agentsDir: string;
  /** Path-scoped instruction files (`.claude/rules`). */
  rulesDir: string | null;
  /** MCP servers are configured in. */
  mcpFile: string;
  /** Settings file(s) shown in the map. */
  settingsFile: string;
  /** Turning off one skill: which scopes the tool honors. */
  skillToggle: "both" | "personalOnly";
  /** Whether one subagent can be turned off for a project. */
  agentToggle: boolean;
  /** Folder the app creates worktrees in, relative to the main worktree. */
  worktreesDir: string;
}

export const TOOLS: Record<ToolId, ToolInfo> = {
  claude: {
    id: "claude",
    name: "Claude Code",
    instructions: { team: "CLAUDE.md", personal: "CLAUDE.local.md" },
    skillsDir: ".claude/skills",
    agentsDir: ".claude/agents",
    rulesDir: ".claude/rules",
    mcpFile: ".mcp.json",
    settingsFile: "settings.json",
    skillToggle: "both",
    agentToggle: true,
    worktreesDir: ".claude/worktrees",
  },
  codex: {
    id: "codex",
    name: "Codex",
    // Codex has no personal file next to AGENTS.md, which is the team's; the
    // composer writes Codex instructions for the team only.
    instructions: { team: "AGENTS.md", personal: "AGENTS.md" },
    skillsDir: ".agents/skills",
    agentsDir: ".codex/agents",
    rulesDir: null,
    mcpFile: "config.toml",
    settingsFile: "config.toml",
    // Codex honors `[[skills.config]]` only in the user's config.toml.
    skillToggle: "personalOnly",
    agentToggle: false,
    worktreesDir: ".codex/worktrees",
  },
};

export const TOOL_IDS = Object.keys(TOOLS) as ToolId[];

export function isToolId(value: unknown): value is ToolId {
  return typeof value === "string" && value in TOOLS;
}
