import { BookOpen, Bot, FileText, type LucideIcon, Ruler, Sparkles } from "lucide-react";
import type { MessageKey } from "../../i18n";
import type { BlockKind, Target } from "../../store/composer";

/** Message keys; translate with `t(KIND_LABEL[kind])`. */
export const KIND_LABEL: Record<BlockKind, MessageKey> = {
  instruction: "composer.kind.instruction",
  rule: "composer.kind.rule",
  skill: "composer.kind.skill",
  agent: "composer.kind.agent",
};

export const KIND_ICON: Record<BlockKind, LucideIcon> = {
  instruction: BookOpen,
  rule: Ruler,
  skill: Sparkles,
  agent: Bot,
};

export const kindColor = (kind: BlockKind) => `var(--c-kind-${kind})`;

/** `label` is a message key. */
export const TARGETS: { target: Target; label: MessageKey; icon: LucideIcon }[] = [
  { target: "claudeMd", label: "composer.target.claudeMd", icon: FileText },
  { target: "rules", label: "composer.target.rules", icon: Ruler },
  { target: "skills", label: "composer.target.skills", icon: Sparkles },
  { target: "agents", label: "composer.target.agents", icon: Bot },
];

export function targetTitle(target: Target, personal: boolean): string {
  switch (target) {
    case "claudeMd":
      return personal ? "CLAUDE.local.md" : "CLAUDE.md";
    case "rules":
      return ".claude/rules";
    case "skills":
      return ".claude/skills";
    case "agents":
      return ".claude/agents";
  }
}
