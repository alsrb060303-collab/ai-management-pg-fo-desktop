import { ask } from "@tauri-apps/plugin-dialog";
import {
  ArrowDown,
  ArrowUp,
  Check,
  Clock,
  GitBranch,
  GitCommitHorizontal,
  GitPullRequest,
  type LucideIcon,
  RefreshCw,
  ShieldCheck,
  TriangleAlert,
  X,
} from "lucide-react";
import type { ReactNode } from "react";
import { IconButton } from "../../components/ui";
import { useT } from "../../i18n";
import { type Checks, type Pull, useGitHubStore } from "../../store/github";
import { useWorkStore } from "../../store/work";
import { open, pullStyle } from "./GitHubSection";
import { useWorkUi } from "./ui";

/** Operations with a label; others show git's own name. */
const OPERATIONS = ["rebase", "merge", "cherry-pick", "revert", "bisect"] as const;
type Operation = (typeof OPERATIONS)[number];
const isOperation = (op: string): op is Operation => (OPERATIONS as readonly string[]).includes(op);

export type Tone = "done" | "wait" | "warn" | "bad" | "idle" | "open" | "merged" | "draft";

const TONE: Record<Tone, { dot: string; text: string }> = {
  done: { dot: "border-success bg-success text-surface", text: "text-success" },
  wait: { dot: "border-accent bg-accent-soft text-accent", text: "text-accent" },
  warn: { dot: "border-warning bg-warning/15 text-warning", text: "text-warning" },
  bad: { dot: "border-danger bg-danger/15 text-danger", text: "text-danger" },
  idle: { dot: "border-border-strong bg-surface text-text-subtle", text: "text-text-subtle" },
  open: { dot: "border-success bg-success/15 text-success", text: "text-success" },
  merged: { dot: "border-merged bg-merged text-surface", text: "text-merged" },
  draft: { dot: "border-border-strong bg-sunken text-text-muted", text: "text-text-muted" },
};

export interface Step {
  key: string;
  label: string;
  detail: string;
  tone: Tone;
  Icon: LucideIcon;
  onClick?: () => void;
}

export interface Next {
  /** A sentence when there is nothing to press (or next to the button). */
  note?: string;
  noteTone?: "muted" | "warn" | "bad" | "good";
  button?: { label: string; Icon: LucideIcon; onClick: () => void; tone?: "primary" | "danger" };
  /** A quieter second choice (e.g. "Abort" next to "Continue"). */
  extra?: { label: string; onClick: () => void };
}

/**
 * Where the checked-out branch stands, as steps from changed files to a
 * merged pull request, with the one thing to do next as a button.
 */
export function FlowCard({ compact }: { compact: boolean }) {
  const t = useT();
  const { overview, busy, run, refresh, select } = useWorkStore();
  const { status, view } = useGitHubStore();
  const reloadGitHub = useGitHubStore((s) => s.load);
  const { setTab, setCreatingPull, focusCommit } = useWorkUi();
  if (!overview) return null;

  const { head, operation, changeCount, unpushed, base, hasRemote } = overview;
  const branch = head.branch;
  const conflicts = overview.changes.filter((c) => c.kind === "conflicted").length;
  const firstPush = hasRemote && !!branch && !!head.commit && !head.upstream;
  const signedIn = !!status?.account && !status.offline;
  const github = signedIn && view && branch ? view : null;
  const isDefault = !!github && branch === github.defaultBranch;
  const pull = github?.current ?? null;
  const checks = github?.checks ?? null;
  const baseBranch = base?.name.replace(/^origin\//, "") ?? github?.defaultBranch ?? null;

  const push = async () => {
    if (await run("work_push", {})) void reloadGitHub(useGitHubStore.getState().project);
  };
  const showConflicts = () => {
    const first = overview.changes.find((c) => c.kind === "conflicted");
    setTab("changes");
    if (first && useWorkStore.getState().selected?.path !== first.path) void select(first);
  };
  const openPullForm = () => {
    setTab("github");
    setCreatingPull(true);
  };

  // Steps
  const steps: Step[] = [
    {
      key: "commit",
      label: t("work.flow.commit"),
      detail: changeCount ? t("work.flow.changes", { n: changeCount }) : t("work.flow.clean"),
      tone: conflicts ? "bad" : changeCount ? "wait" : "done",
      Icon: GitCommitHorizontal,
      onClick: () => setTab("changes"),
    },
    {
      key: "push",
      label: t("work.flow.push"),
      ...(!hasRemote
        ? { detail: t("work.flow.noRemote"), tone: "idle" as Tone }
        : head.behind > 0
          ? { detail: t("work.flow.behind", { n: head.behind }), tone: "warn" as Tone }
          : firstPush
            ? { detail: t("work.flow.firstPush"), tone: "wait" as Tone }
            : unpushed > 0
              ? { detail: t("work.flow.waiting", { n: unpushed }), tone: "wait" as Tone }
              : { detail: t("work.flow.pushed"), tone: "done" as Tone }),
      Icon: ArrowUp,
      onClick: () => setTab("history"),
    },
  ];
  if (github && !isDefault) steps.push(pullStep(t, pull, () => setTab("github")));
  if (github && (pull || github.pushed)) steps.push(ciStep(t, checks, pull));

  // What to do next, most urgent first.
  const next: Next = (() => {
    if (operation && operation !== "bisect") {
      const op = isOperation(operation) ? t(`work.op.${operation}`) : operation;
      const abort = {
        label: t("work.next.abort"),
        onClick: async () => {
          if (
            await ask(t("work.next.abortConfirm", { op }), { title: op, kind: "warning" }).catch(
              () => false,
            )
          )
            void run("work_abort", {});
        },
      };
      if (conflicts)
        return {
          note: t("work.next.conflicts", { n: conflicts }),
          noteTone: "bad",
          button: {
            label: t("work.next.showConflicts"),
            Icon: TriangleAlert,
            onClick: showConflicts,
          },
          extra: abort,
        };
      return {
        note: t("work.next.continueReady"),
        noteTone: "warn",
        button: {
          label: t("work.next.continue"),
          Icon: Check,
          onClick: () => void run("work_continue", {}),
        },
        extra: abort,
      };
    }
    if (operation)
      return {
        note: t("work.next.operation", {
          op: isOperation(operation) ? t(`work.op.${operation}`) : operation,
        }),
        noteTone: "warn",
      };
    if (conflicts)
      return {
        note: t("work.next.conflicts", { n: conflicts }),
        noteTone: "bad",
        button: {
          label: t("work.next.showConflicts"),
          Icon: TriangleAlert,
          onClick: showConflicts,
        },
      };
    if (changeCount)
      return {
        button: {
          label: t("work.next.commit", { n: changeCount }),
          Icon: GitCommitHorizontal,
          onClick: focusCommit,
        },
      };
    if (!head.commit) return {};
    if (!hasRemote) return { note: t("work.next.noRemote"), noteTone: "muted" };
    if (head.behind > 0)
      return {
        button: {
          label: t("work.next.pull", { n: head.behind }),
          Icon: ArrowDown,
          onClick: () => void run("work_pull", {}),
        },
      };
    if (branch && (firstPush || unpushed > 0))
      return {
        button: {
          label: firstPush ? t("work.next.firstPush") : t("work.next.push", { n: unpushed }),
          Icon: ArrowUp,
          onClick: () => void push(),
        },
      };
    if (status?.configured && !signedIn && !status.offline)
      return {
        note: t("work.next.signIn"),
        noteTone: "muted",
        button: {
          label: t("work.next.signInButton"),
          Icon: GitPullRequest,
          onClick: () => setTab("github"),
        },
      };
    if (github && !isDefault)
      return pullNext(t, pull, checks, {
        create: openPullForm,
        base: baseBranch,
        backToBase: (b) => void run("work_switch", { branch: b }),
      });
    return { note: t("work.next.done"), noteTone: "good" };
  })();

  return (
    <FlowShell
      compact={compact}
      branch={branch ?? t("work.detached", { commit: head.commit ?? "" })}
      pull={pull}
      base={base}
      steps={steps}
      next={next}
      busy={busy}
      onRefresh={() => void refresh(true)}
    />
  );
}

/** The card itself, shared with the GitHub-only repository view. */
export function FlowShell({
  compact,
  branch,
  branchSlot,
  pull,
  base,
  steps,
  next,
  busy,
  onRefresh,
}: {
  compact: boolean;
  branch: string;
  /** Replaces the branch name (e.g. with a branch picker). */
  branchSlot?: ReactNode;
  pull: Pull | null;
  base: { name: string; ahead: number; behind: number } | null;
  steps: Step[];
  next: Next;
  busy: boolean;
  onRefresh: () => void;
}) {
  const t = useT();
  return (
    <section className="rounded-xl border border-border bg-surface-raised p-3 shadow-card">
      <div className="flex items-center gap-2">
        <GitBranch size={compact ? 16 : 18} className="shrink-0 text-accent" />
        <span
          className={`min-w-0 truncate font-semibold ${compact ? "text-sm" : "text-base"}`}
          title={branch}
        >
          {branchSlot ?? branch}
        </span>
        {pull && <PullChip pull={pull} />}
        <IconButton
          label={t("work.refresh")}
          disabled={busy}
          onClick={onRefresh}
          className="ml-auto size-7 shrink-0"
        >
          <RefreshCw size={14} className={busy ? "animate-spin" : undefined} />
        </IconButton>
      </div>
      {base && <BaseLine base={base} />}

      <ol className="mt-3 flex items-start">
        {steps.map((step, i) => (
          <StepNode key={step.key} step={step} first={i === 0} prev={steps[i - 1]} />
        ))}
      </ol>

      {(next.note || next.button) && (
        <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border pt-3">
          {next.note && (
            <p
              className={`min-w-0 flex-1 text-xs ${
                next.noteTone === "warn"
                  ? "text-warning"
                  : next.noteTone === "bad"
                    ? "text-danger"
                    : next.noteTone === "good"
                      ? "text-success"
                      : "text-text-muted"
              }`}
            >
              {next.noteTone === "warn" || next.noteTone === "bad" ? (
                <TriangleAlert size={12} className="mr-1 inline align-[-2px]" />
              ) : null}
              {next.note}
            </p>
          )}
          {next.extra && (
            <button
              type="button"
              disabled={busy}
              onClick={next.extra.onClick}
              className="shrink-0 rounded-lg border border-border px-3 py-1.5 text-xs text-text-muted hover:border-border-strong hover:text-text disabled:opacity-50"
            >
              {next.extra.label}
            </button>
          )}
          {next.button && (
            <button
              type="button"
              disabled={busy}
              onClick={next.button.onClick}
              className={`flex items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium disabled:opacity-50 ${
                next.note || next.extra ? "shrink-0" : "w-full"
              } ${
                next.button.tone === "danger"
                  ? "bg-danger text-surface hover:opacity-90"
                  : "bg-primary text-primary-fg hover:bg-primary-hover"
              }`}
            >
              <next.button.Icon size={13} />
              {next.button.label}
            </button>
          )}
        </div>
      )}
    </section>
  );
}

type T = ReturnType<typeof useT>;

/** The PR step: its number and state in GitHub's colors. */
export function pullStep(t: T, pull: Pull | null, onNoPull: () => void): Step {
  const style = pull ? pullStyle(pull) : null;
  return {
    key: "pr",
    label: t("work.flow.pr"),
    detail: pull
      ? `#${pull.number} ${t(style?.key === "draft" ? "work.github.draft" : `work.github.state.${pull.state}`)}`
      : t("work.flow.noPull"),
    tone: (style?.key as Tone | undefined) ?? "idle",
    Icon: style?.Icon ?? GitPullRequest,
    onClick: () => (pull ? open(pull.url) : onNoPull()),
  };
}

export function ciStep(t: T, checks: Checks | null, pull: Pull | null): Step {
  return {
    key: "ci",
    label: t("work.flow.ci"),
    ...(!checks
      ? { detail: t("work.flow.noCi"), tone: "idle" as Tone, Icon: ShieldCheck }
      : checks.failed
        ? { detail: t("work.flow.ciFailed", { n: checks.failed }), tone: "bad" as Tone, Icon: X }
        : checks.pending
          ? { detail: t("work.flow.ciPending"), tone: "warn" as Tone, Icon: Clock }
          : { detail: t("work.flow.ciPassed"), tone: "done" as Tone, Icon: Check }),
    onClick: pull ? () => open(`${pull.url}/checks`) : undefined,
  };
}

/** What to do next once the branch is on GitHub: open a PR, fix CI, review, or go back. */
export function pullNext(
  t: T,
  pull: Pull | null,
  checks: Checks | null,
  on: { create: () => void; base: string | null; backToBase: (base: string) => void },
): Next {
  if (!pull || pull.state === "closed")
    return { button: { label: t("work.next.createPr"), Icon: GitPullRequest, onClick: on.create } };
  if (pull.state === "merged") {
    const base = on.base;
    return {
      note: t("work.next.merged"),
      noteTone: "good",
      button: base
        ? {
            label: t("work.next.backToBase", { base }),
            Icon: GitBranch,
            onClick: () => on.backToBase(base),
          }
        : undefined,
    };
  }
  if (checks?.failed)
    return {
      button: {
        label: t("work.next.ciFailed"),
        Icon: X,
        tone: "danger",
        onClick: () => open(`${pull.url}/checks`),
      },
    };
  if (checks?.pending) return { note: t("work.next.ciPending"), noteTone: "warn" };
  return {
    note: t("work.next.reviewHint"),
    noteTone: "muted",
    button: { label: t("work.next.review"), Icon: GitPullRequest, onClick: () => open(pull.url) },
  };
}

function PullChip({ pull }: { pull: Pull }) {
  const s = pullStyle(pull);
  return (
    <button
      type="button"
      onClick={() => open(pull.url)}
      className={`flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium ${s.chip} ${s.text} hover:opacity-80`}
    >
      <s.Icon size={12} />#{pull.number}
    </button>
  );
}

function BaseLine({ base }: { base: { name: string; ahead: number; behind: number } }) {
  const t = useT();
  const parts: ReactNode[] = [];
  if (base.ahead)
    parts.push(
      <span key="a" className="flex items-center gap-0.5 text-accent">
        <ArrowUp size={11} />
        {t("work.flow.ahead", { n: base.ahead })}
      </span>,
    );
  if (base.behind)
    parts.push(
      <span key="b" className="flex items-center gap-0.5 text-warning">
        <ArrowDown size={11} />
        {t("work.flow.behindBase", { n: base.behind })}
      </span>,
    );
  return (
    <p
      className="mt-1 flex flex-wrap items-center gap-x-2 pl-6 text-[11px] text-text-subtle"
      title={t("work.flow.baseTitle", { base: base.name, ahead: base.ahead, behind: base.behind })}
    >
      {parts.length ? (
        <>
          <span>{t("work.flow.base", { base: base.name })}</span>
          {parts}
        </>
      ) : (
        t("work.flow.sameAsBase", { base: base.name })
      )}
    </p>
  );
}

function StepNode({ step, first, prev }: { step: Step; first: boolean; prev?: Step }) {
  const tone = TONE[step.tone];
  const lineDone = prev && (prev.tone === "done" || prev.tone === "merged");
  const inner = (
    <>
      <span className="relative flex w-full items-center justify-center">
        {!first && (
          <span
            className={`absolute top-1/2 right-1/2 left-[-50%] h-0.5 -translate-y-1/2 ${
              lineDone ? "bg-success/60" : "bg-border-strong"
            }`}
          />
        )}
        <span
          className={`relative z-10 flex size-7 items-center justify-center rounded-full border-2 ${tone.dot}`}
        >
          <step.Icon size={13} strokeWidth={2.5} />
        </span>
      </span>
      <span className="mt-1 text-[11px] font-medium text-text">{step.label}</span>
      <span className={`max-w-full truncate px-0.5 text-[10.5px] leading-tight ${tone.text}`}>
        {step.detail}
      </span>
    </>
  );
  return (
    <li className="flex min-w-0 flex-1 flex-col items-center text-center">
      {step.onClick ? (
        <button
          type="button"
          onClick={step.onClick}
          className="flex w-full flex-col items-center rounded-lg pb-0.5 hover:bg-hover"
        >
          {inner}
        </button>
      ) : (
        <div className="flex w-full flex-col items-center pb-0.5">{inner}</div>
      )}
    </li>
  );
}
