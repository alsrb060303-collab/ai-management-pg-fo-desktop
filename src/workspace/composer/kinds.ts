import {
  BookOpen,
  Bot,
  FilePen,
  FileText,
  Flag,
  type LucideIcon,
  Ruler,
  Sparkles,
  Webhook,
} from "lucide-react";
import type { MessageKey } from "../../i18n";
import type { BlockKind, Target } from "../../store/composer";
import { TOOLS, type ToolId } from "../../tools";

/** Message keys; translate with `t(KIND_LABEL[kind])`. */
export const KIND_LABEL: Record<BlockKind, MessageKey> = {
  instruction: "composer.kind.instruction",
  rule: "composer.kind.rule",
  skill: "composer.kind.skill",
  agent: "composer.kind.agent",
  hook: "composer.kind.hook",
};

export const KIND_ICON: Record<BlockKind, LucideIcon> = {
  instruction: BookOpen,
  rule: Ruler,
  skill: Sparkles,
  agent: Bot,
  hook: Webhook,
};

export const kindColor = (kind: BlockKind | "plan") => `var(--c-kind-${kind})`;

/**
 * `label` is a message key; `null` for the instructions output, which is
 * named after the tool's file (`instructionsLabel`).
 */
export const TARGETS: { target: Target; label: MessageKey | null; icon: LucideIcon }[] = [
  { target: "instructions", label: null, icon: FileText },
  { target: "rules", label: "composer.target.rules", icon: Ruler },
  { target: "skills", label: "composer.target.skills", icon: Sparkles },
  { target: "agents", label: "composer.target.agents", icon: Bot },
  { target: "hooks", label: "composer.target.hooks", icon: Webhook },
  { target: "plans", label: "composer.target.plans", icon: Flag },
  { target: "custom", label: "composer.target.custom", icon: FilePen },
];

/** The instructions output's palette name: its file without `.md`, uppercase (CLAUDE, AGENTS). */
export function instructionsLabel(tool: ToolId): string {
  return TOOLS[tool].instructions.team.replace(/\.md$/i, "").toUpperCase();
}

/** The hooks file of `tool` (as in compile.rs); Codex has no personal one. */
export function hooksFile(tool: ToolId, personal: boolean): string {
  if (tool === "codex") return ".codex/hooks.json";
  return personal ? ".claude/settings.local.json" : ".claude/settings.json";
}

/** The file or folder an output writes for `tool`; custom outputs show their path. */
export function targetTitle(
  target: Target,
  personal: boolean,
  tool: ToolId,
  path?: string,
): string {
  const info = TOOLS[tool];
  switch (target) {
    case "instructions":
      return personal ? info.instructions.personal : info.instructions.team;
    case "rules":
      return info.rulesDir ?? ".claude/rules";
    case "skills":
      return info.skillsDir;
    case "agents":
      return info.agentsDir;
    case "hooks":
      return hooksFile(tool, personal);
    case "plans":
      return personal ? "PLANS.local.md" : "PLANS.md";
    case "custom":
      return path?.trim() || "…";
  }
}
