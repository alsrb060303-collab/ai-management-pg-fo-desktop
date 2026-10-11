import { BookOpen, Bot, FileText, type LucideIcon, Ruler, Sparkles } from "lucide-react";
import type { BlockKind, Target } from "../../store/composer";

export const KIND_LABEL: Record<BlockKind, string> = {
  instruction: "지침",
  rule: "규칙",
  skill: "Skill",
  agent: "Subagent",
};

export const KIND_ICON: Record<BlockKind, LucideIcon> = {
  instruction: BookOpen,
  rule: Ruler,
  skill: Sparkles,
  agent: Bot,
};

export const kindColor = (kind: BlockKind) => `var(--c-kind-${kind})`;

export const TARGETS: { target: Target; label: string; hint: string; icon: LucideIcon }[] = [
  { target: "claudeMd", label: "CLAUDE.md", hint: "항상 읽히는 지침", icon: FileText },
  { target: "rules", label: "규칙", hint: ".claude/rules", icon: Ruler },
  { target: "skills", label: "Skills", hint: ".claude/skills", icon: Sparkles },
  { target: "agents", label: "Subagents", hint: ".claude/agents", icon: Bot },
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
