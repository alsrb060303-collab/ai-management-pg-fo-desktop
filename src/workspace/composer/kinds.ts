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

/** `label` is a message key. */
export const TARGETS: { target: Target; label: MessageKey; icon: LucideIcon }[] = [
  { target: "claudeMd", label: "composer.target.claudeMd", icon: FileText },
  { target: "rules", label: "composer.target.rules", icon: Ruler },
  { target: "skills", label: "composer.target.skills", icon: Sparkles },
  { target: "agents", label: "composer.target.agents", icon: Bot },
  { target: "hooks", label: "composer.target.hooks", icon: Webhook },
  { target: "plans", label: "composer.target.plans", icon: Flag },
  { target: "custom", label: "composer.target.custom", icon: FilePen },
];

/** The file or folder an output writes; custom outputs show their path. */
export function targetTitle(target: Target, personal: boolean, path?: string): string {
  switch (target) {
    case "claudeMd":
      return personal ? "CLAUDE.local.md" : "CLAUDE.md";
    case "rules":
      return ".claude/rules";
    case "skills":
      return ".claude/skills";
    case "agents":
      return ".claude/agents";
    case "hooks":
      return personal ? ".claude/settings.local.json" : ".claude/settings.json";
    case "plans":
      return personal ? "PLANS.local.md" : "PLANS.md";
    case "custom":
      return path?.trim() || "…";
  }
}
