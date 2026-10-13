import { CircleCheck, CircleDashed, CircleDot } from "lucide-react";
import type { MessageKey } from "../../i18n";
import type { PlanAction, PlanItem, PlanStatus } from "../../store/composer";

/** Palette entries: a main goal makes a plan node, the rest go into one. */
export type PlanEntry = "main" | "sub" | "detail" | "step";

export const PLAN_ENTRIES: { entry: PlanEntry; label: MessageKey; hint: MessageKey }[] = [
  { entry: "main", label: "composer.plan.main", hint: "composer.plan.mainHint" },
  { entry: "sub", label: "composer.plan.sub", hint: "composer.plan.subHint" },
  { entry: "detail", label: "composer.plan.detail", hint: "composer.plan.detailHint" },
  { entry: "step", label: "composer.plan.step", hint: "composer.plan.stepHint" },
];

export const STATUS: Record<
  PlanStatus,
  { label: MessageKey; icon: typeof CircleCheck; tone: string }
> = {
  n: { label: "composer.plan.status.n", icon: CircleDashed, tone: "text-text-subtle" },
  p: { label: "composer.plan.status.p", icon: CircleDot, tone: "text-accent" },
  y: { label: "composer.plan.status.y", icon: CircleCheck, tone: "text-success" },
};

/** Clicking the status moves it along: not yet → in progress → done → not yet. */
export const nextStatus = (s: PlanStatus | undefined): PlanStatus =>
  s === "p" ? "y" : s === "y" ? "n" : "p";

export const ACTIONS: { action: PlanAction; label: MessageKey }[] = [
  { action: "text", label: "composer.plan.action.text" },
  { action: "agent", label: "composer.plan.action.agent" },
  { action: "skill", label: "composer.plan.action.skill" },
  { action: "ask", label: "composer.plan.action.ask" },
];

/**
 * Each row's number (`2`, `2.1`) and indent level: 0 beside the main goal,
 * 1 under a sub-goal, 2 under a detail goal. Steps take the level of the goal
 * above them. Same reading as `plans::render`.
 */
export function layout(items: PlanItem[]): { number: string; level: number }[] {
  let sub = 0;
  let detail = 0;
  let level = 0;
  return items.map((item) => {
    if (item.kind === "sub") {
      sub += 1;
      detail = 0;
      level = 1;
      return { number: `${sub}`, level: 0 };
    }
    if (item.kind === "detail") {
      detail += 1;
      level = 2;
      return { number: `${sub}.${detail}`, level: 1 };
    }
    return { number: "", level };
  });
}

/** Where a drop at `clientY` lands among the plan node's rows. */
export function dropIndex(planEl: Element, clientY: number): number {
  const rows = Array.from(planEl.querySelectorAll("[data-plan-row]"));
  const index = rows.findIndex((el) => {
    const r = el.getBoundingClientRect();
    return clientY < r.top + r.height / 2;
  });
  return index === -1 ? rows.length : index;
}
