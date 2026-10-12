import { ask } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  ArrowDown,
  ArrowUp,
  FolderOpen,
  FolderTree,
  GitBranch,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { IconButton } from "../../components/ui";
import { timeAgo, useT } from "../../i18n";
import { useGitHubStore } from "../../store/github";
import { useProjectStore } from "../../store/project";
import { useRemoteStore } from "../../store/remote";
import {
  type Branch,
  type Change,
  type ChangeKind,
  type Diff,
  useWorkStore,
  type Worktree,
} from "../../store/work";
import { GitHubSection } from "./GitHubSection";
import { GitSetupGuide } from "./GitSetupGuide";
import { RemoteRepo, RepoPicker } from "./RemoteRepo";

/** How often the state is re-read while the panel is visible. */
const POLL_MS = 4000;
/** Branches shown before "Show more". */
const BRANCH_LIMIT = 12;

const confirm = (message: string, title: string, danger = false) =>
  ask(message, { title, kind: danger ? "warning" : "info" }).catch(() => false);

/**
 * The open project's branch, changed files, branches and worktrees. Shared by
 * the sidebar (`compact`) and the main window's work page.
 */
export function WorkPanel({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const project = useProjectStore((s) => s.current);
  const choose = useProjectStore((s) => s.choose);
  const { overview, notGit, gitMissing, loading, error, load, refresh, dismissError } =
    useWorkStore();
  const remote = useRemoteStore((s) => s.slug);

  useEffect(() => {
    void load(project);
  }, [project, load]);

  // Follow what Claude Code and the terminal change while the panel is on screen.
  useEffect(() => {
    const tick = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    const focus = () => void refresh(true);
    const timer = setInterval(tick, POLL_MS);
    window.addEventListener("focus", focus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", focus);
    };
  }, [refresh]);

  // A repository picked from GitHub that has no clone here.
  if (remote) {
    return (
      <div className={compact ? "space-y-4" : "space-y-6"}>
        <RepoPicker compact={compact} />
        <RemoteRepo compact={compact} />
      </div>
    );
  }

  if (!project) {
    return (
      <div className="space-y-3 text-sm">
        <RepoPicker compact={compact} />
        <p className="text-text-muted">{t("work.noProject")}</p>
        <button
          type="button"
          onClick={() => void choose()}
          className="rounded-lg border border-border bg-surface-raised px-3 py-1.5 text-sm hover:border-border-strong"
        >
          {t("work.chooseFolder")}
        </button>
        <GitHubSection compact={compact} />
      </div>
    );
  }

  return (
    <div className={compact ? "space-y-4" : "space-y-6"}>
      <RepoPicker compact={compact} />
      {error && (
        <div className="flex items-start gap-2 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">
          <p className="selectable min-w-0 flex-1 whitespace-pre-wrap break-words">{error}</p>
          <button
            type="button"
            aria-label={t("work.close")}
            onClick={dismissError}
            className="shrink-0"
          >
            <X size={13} />
          </button>
        </div>
      )}
      {loading && !overview && !notGit && (
        <p className="text-xs text-text-muted">{t("work.loading")}</p>
      )}
      {notGit && (
        <GitSetupGuide
          key={project}
          stage="none"
          project={project}
          branch="main"
          hasRemote={false}
          gitMissing={gitMissing}
          compact={compact}
        />
      )}
      {!overview && <GitHubSection compact={compact} />}
      {overview && (
        <>
          <HeadBar compact={compact} />
          {(!overview.head.commit || !overview.hasRemote) && (
            <GitSetupGuide
              key={`${project}:${overview.head.commit ? "noRemote" : "noCommit"}`}
              stage={overview.head.commit ? "noRemote" : "noCommit"}
              project={project}
              branch={overview.head.branch ?? "main"}
              hasRemote={overview.hasRemote}
              gitMissing={false}
              compact={compact}
            />
          )}
          <GitHubSection compact={compact} />
          <Changes compact={compact} />
          <Branches compact={compact} />
          <Worktrees compact={compact} />
        </>
      )}
    </div>
  );
}

/** Operations with a label; others show git's own name. */
const OPERATIONS = ["rebase", "merge", "cherry-pick", "revert", "bisect"] as const;
type Operation = (typeof OPERATIONS)[number];
const isOperation = (op: string): op is Operation => (OPERATIONS as readonly string[]).includes(op);

function HeadBar({ compact }: { compact: boolean }) {
  const t = useT();
  const { overview, busy, refresh } = useWorkStore();
  if (!overview) return null;
  const { head, operation } = overview;
  return (
    <div className="flex items-center gap-2">
      <GitBranch size={compact ? 15 : 17} className="shrink-0 text-text-subtle" />
      <span className={`min-w-0 truncate font-semibold ${compact ? "text-sm" : "text-base"}`}>
        {head.branch ?? t("work.detached", { commit: head.commit ?? "" })}
      </span>
      <Track ahead={head.ahead} behind={head.behind} />
      {operation && (
        <span className="shrink-0 rounded-full bg-warning/15 px-2 py-px text-[11px] text-warning">
          {isOperation(operation) ? t(`work.op.${operation}`) : operation}
        </span>
      )}
      {!head.commit && <span className="text-xs text-text-subtle">{t("work.noCommits")}</span>}
      <SyncButtons compact={compact} />
      <IconButton
        label={t("work.refresh")}
        disabled={busy}
        onClick={() => void refresh(true)}
        className="ml-auto size-7"
      >
        <RefreshCw size={14} />
      </IconButton>
    </div>
  );
}

/** Push (and set the upstream the first time) and fast-forward pull. */
function SyncButtons({ compact }: { compact: boolean }) {
  const t = useT();
  const { overview, busy, run } = useWorkStore();
  const reloadGitHub = useGitHubStore((s) => s.load);
  if (!overview?.head.branch || !overview.head.commit || !overview.hasRemote) return null;
  const { ahead, behind, upstream } = overview.head;
  const canPush = ahead > 0 || !upstream;
  const button =
    "flex shrink-0 items-center gap-1 rounded-md border border-border bg-surface-raised px-2 py-0.5 text-[11px] hover:border-border-strong disabled:opacity-50";
  return (
    <span className="flex shrink-0 items-center gap-1">
      {behind > 0 && (
        <button
          type="button"
          disabled={busy}
          onClick={() => void run("work_pull", {})}
          title={t("work.pull.title")}
          className={button}
        >
          <ArrowDown size={11} />
          {!compact && t("work.pull")}
        </button>
      )}
      {canPush && (
        <button
          type="button"
          disabled={busy}
          onClick={async () => {
            if (await run("work_push", {})) {
              void reloadGitHub(useGitHubStore.getState().project);
            }
          }}
          title={upstream ? t("work.push.title") : t("work.push.first")}
          className={button}
        >
          <ArrowUp size={11} />
          {!compact && t("work.push")}
        </button>
      )}
    </span>
  );
}

function Track({ ahead, behind, gone }: { ahead: number; behind: number; gone?: boolean }) {
  const t = useT();
  if (gone)
    return <span className="shrink-0 text-[11px] text-text-subtle">{t("work.track.gone")}</span>;
  if (!ahead && !behind) return null;
  return (
    <span
      className="flex shrink-0 items-center gap-1 text-[11px] text-text-muted"
      title={t("work.track.title", { ahead, behind })}
    >
      {ahead > 0 && (
        <span className="flex items-center">
          <ArrowUp size={11} />
          {ahead}
        </span>
      )}
      {behind > 0 && (
        <span className="flex items-center">
          <ArrowDown size={11} />
          {behind}
        </span>
      )}
    </span>
  );
}

function Section({
  title,
  count,
  action,
  children,
}: {
  title: string;
  count?: number;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section>
      <h3 className="mb-1.5 flex h-6 items-center gap-2 text-xs font-semibold">
        {title}
        {count !== undefined && <span className="font-normal text-text-subtle">{count}</span>}
        <span className="ml-auto flex items-center">{action}</span>
      </h3>
      {children}
    </section>
  );
}

/** Labels are `work.kind.<kind>`. */
const KIND: Record<ChangeKind, { letter: string; color: string }> = {
  modified: { letter: "M", color: "text-warning" },
  added: { letter: "A", color: "text-success" },
  deleted: { letter: "D", color: "text-danger" },
  renamed: { letter: "R", color: "text-info" },
  copied: { letter: "C", color: "text-info" },
  typeChanged: { letter: "T", color: "text-warning" },
  untracked: { letter: "U", color: "text-success" },
  conflicted: { letter: "!", color: "text-danger" },
};

function Changes({ compact }: { compact: boolean }) {
  const t = useT();
  const { overview, selected, diff, select } = useWorkStore();
  if (!overview) return null;
  const { changes, changeCount } = overview;
  return (
    <Section title={t("work.changes")} count={changeCount}>
      {changeCount === 0 ? (
        <p className="text-xs text-text-subtle">{t("work.changes.none")}</p>
      ) : (
        <ul className="space-y-px">
          {changes.map((c) => (
            <li key={`${c.kind}:${c.path}`}>
              <ChangeRow
                change={c}
                compact={compact}
                open={selected?.path === c.path}
                onClick={() => void select(c)}
              />
              {selected?.path === c.path && <DiffView diff={diff} compact={compact} />}
            </li>
          ))}
          {changeCount > changes.length && (
            <li className="px-2 pt-1 text-[11px] text-text-subtle">
              {t("work.changes.more", { n: changeCount - changes.length })}
            </li>
          )}
        </ul>
      )}
      {changeCount > 0 && overview.operation === null && <CommitBox />}
    </Section>
  );
}

/** A message and "Commit": every change, or only the staged ones. */
function CommitBox() {
  const t = useT();
  const { overview, busy, run } = useWorkStore();
  const [message, setMessage] = useState("");
  const someStaged = overview?.changes.some((c) => c.staged) ?? false;
  const [all, setAll] = useState(true);
  if (!overview) return null;
  const onlyStaged = someStaged && !all;
  return (
    <form
      onSubmit={async (e) => {
        e.preventDefault();
        if (await run("work_commit", { message, all: !onlyStaged })) setMessage("");
      }}
      className="mt-2 space-y-1.5"
    >
      <textarea
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) e.currentTarget.form?.requestSubmit();
        }}
        placeholder={t("work.commit.message")}
        rows={2}
        className="w-full min-w-0 resize-y rounded-md border border-border bg-surface px-2 py-1 text-xs outline-none placeholder:text-text-subtle focus:border-border-strong"
      />
      <div className="flex items-center gap-2 text-xs">
        {someStaged && (
          <label className="flex items-center gap-1.5 text-text-muted">
            <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />
            {t("work.commit.all")}
          </label>
        )}
        <button
          type="submit"
          disabled={busy || !message.trim()}
          title="Ctrl+Enter"
          className="ml-auto rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          {t(onlyStaged ? "work.commit.staged" : "work.commit")}
        </button>
      </div>
    </form>
  );
}

function ChangeRow({
  change,
  compact,
  open,
  onClick,
}: {
  change: Change;
  compact: boolean;
  open: boolean;
  onClick: () => void;
}) {
  const t = useT();
  const kind = KIND[change.kind];
  const label = t(`work.kind.${change.kind}`);
  const trimmed = change.path.replace(/\/$/, "");
  const slash = trimmed.lastIndexOf("/");
  const dir = slash >= 0 ? trimmed.slice(0, slash + 1) : "";
  const name = trimmed.slice(slash + 1) + (change.path.endsWith("/") ? "/" : "");
  const stage = change.staged ? t(change.unstaged ? "work.partlyStaged" : "work.staged") : "";
  return (
    <button
      type="button"
      onClick={onClick}
      title={`${label}${stage ? ` · ${stage}` : ""}${change.origPath ? ` · ${t("work.renamedFrom", { path: change.origPath })}` : ""}\n${change.path}`}
      className={`flex w-full items-center gap-2 rounded-md px-2 py-1 text-left font-mono ${
        compact ? "text-[11px]" : "text-xs"
      } ${open ? "bg-active" : "hover:bg-hover"}`}
    >
      <span className={`w-3 shrink-0 text-center font-semibold ${kind.color}`}>{kind.letter}</span>
      <span className="min-w-0 flex-1 truncate">
        <span className="text-text-subtle">{dir}</span>
        {name}
      </span>
      {change.staged && (
        <span
          className="size-1.5 shrink-0 rounded-full bg-success"
          role="img"
          aria-label={t("work.staged")}
        />
      )}
    </button>
  );
}

function DiffView({ diff, compact }: { diff: Diff | null; compact: boolean }) {
  const t = useT();
  if (!diff)
    return <p className="px-2 py-2 text-[11px] text-text-subtle">{t("work.diff.loading")}</p>;
  if (diff.binary)
    return <p className="px-2 py-2 text-[11px] text-text-subtle">{t("work.diff.binary")}</p>;
  const lines = diff.text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return (
    <div
      className={`selectable my-1 overflow-auto rounded-lg border border-border bg-sunken font-mono leading-snug ${
        compact ? "max-h-72 text-[10.5px]" : "max-h-[28rem] text-xs"
      }`}
    >
      <pre className="min-w-fit py-1">
        {lines.map((line, i) => (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity
            key={i}
            className={`px-2 ${
              line.startsWith("@@")
                ? "text-info"
                : line.startsWith("+++") || line.startsWith("---")
                  ? "text-text-subtle"
                  : line.startsWith("+")
                    ? "bg-success/10 text-success"
                    : line.startsWith("-")
                      ? "bg-danger/10 text-danger"
                      : "text-text-muted"
            }`}
          >
            {line || " "}
          </div>
        ))}
        {lines.length === 0 && <div className="px-2 text-text-subtle">{t("work.diff.empty")}</div>}
      </pre>
      {diff.truncated && (
        <p className="border-t border-border px-2 py-1 text-[11px] text-text-subtle">
          {t("work.diff.truncated")}
        </p>
      )}
    </div>
  );
}

function Branches({ compact }: { compact: boolean }) {
  const t = useT();
  const { overview, busy, run, checkBranch } = useWorkStore();
  const [adding, setAdding] = useState(false);
  const [all, setAll] = useState(false);
  if (!overview) return null;
  const { branches } = overview;
  const shown = all ? branches : branches.slice(0, BRANCH_LIMIT);

  const remove = async (b: Branch) => {
    const unmerged = await checkBranch(b.name);
    if (unmerged === null) return;
    if (unmerged === 0) {
      if (
        await confirm(
          t("work.branch.deleteConfirm", { name: b.name }),
          t("work.branch.deleteTitle"),
        )
      )
        await run("work_delete_branch", { name: b.name, force: false });
      return;
    }
    const first = await confirm(
      t(unmerged === 1 ? "work.branch.unmergedConfirm.one" : "work.branch.unmergedConfirm", {
        name: b.name,
        n: unmerged,
      }),
      t("work.branch.unmergedTitle"),
      true,
    );
    if (!first) return;
    const second = await confirm(
      t("work.forceDeleteConfirm", { name: b.name }),
      t("work.forceDelete"),
      true,
    );
    if (second) await run("work_delete_branch", { name: b.name, force: true });
  };

  return (
    <Section
      title={t("work.branches")}
      count={branches.length}
      action={
        <IconButton
          label={t("work.branch.new")}
          disabled={busy}
          onClick={() => setAdding(!adding)}
          className="size-6"
        >
          <Plus size={14} />
        </IconButton>
      }
    >
      {adding && <NewBranch onDone={() => setAdding(false)} />}
      <ul className="space-y-px">
        {shown.map((b) => (
          <li
            key={b.name}
            className={`group flex h-7 items-center gap-2 rounded-md px-2 hover:bg-hover ${
              compact ? "text-xs" : "text-sm"
            }`}
            title={`${b.commit} ${b.subject}${
              b.upstream ? `\n${t("work.branch.upstream", { upstream: b.upstream })}` : ""
            }${b.worktree && !b.current ? `\n${t("work.branch.worktree", { path: b.worktree })}` : ""}`}
          >
            <span className={`min-w-0 truncate ${b.current ? "font-semibold" : ""}`}>{b.name}</span>
            {b.current && <Badge>{t("work.branch.current")}</Badge>}
            {b.worktree && !b.current && <Badge>worktree</Badge>}
            <Track ahead={b.ahead} behind={b.behind} gone={b.gone} />
            {!compact && (
              <span className="min-w-0 flex-1 truncate text-xs text-text-subtle">{b.subject}</span>
            )}
            <span className="ml-auto shrink-0 text-[11px] text-text-subtle group-hover:hidden">
              {timeAgo(b.time)}
            </span>
            {!b.current && !b.worktree && (
              <span className="ml-auto hidden shrink-0 items-center gap-1 group-hover:flex">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void run("work_switch", { branch: b.name })}
                  className="rounded px-1.5 py-0.5 text-[11px] text-accent hover:bg-active disabled:opacity-50"
                >
                  {t("work.branch.switch")}
                </button>
                <IconButton
                  label={t("work.deleteNamed", { name: b.name })}
                  disabled={busy}
                  onClick={() => void remove(b)}
                  className="size-5 hover:text-danger"
                >
                  <Trash2 size={12} />
                </IconButton>
              </span>
            )}
          </li>
        ))}
      </ul>
      {branches.length > BRANCH_LIMIT && (
        <button
          type="button"
          onClick={() => setAll(!all)}
          className="mt-1 rounded px-2 py-0.5 text-[11px] text-text-subtle hover:bg-hover hover:text-text"
        >
          {all
            ? t("work.branch.showLess")
            : t("work.branch.showMore", { n: branches.length - BRANCH_LIMIT })}
        </button>
      )}
    </Section>
  );
}

function Badge({ children }: { children: ReactNode }) {
  return (
    <span className="shrink-0 rounded-full bg-sunken px-1.5 py-px text-[10px] text-text-muted">
      {children}
    </span>
  );
}

const inputClass =
  "min-w-0 flex-1 rounded-md border border-border bg-surface-raised px-2 py-1 text-xs outline-none placeholder:text-text-subtle focus:border-border-strong";

function NewBranch({ onDone }: { onDone: () => void }) {
  const t = useT();
  const { overview, busy, run } = useWorkStore();
  const [name, setName] = useState("");
  const [switchTo, setSwitchTo] = useState(true);
  // An unborn branch has no ref to start from yet.
  const current = overview?.branches.find((b) => b.name === overview.head.branch)?.name ?? null;
  const [base, setBase] = useState<string>(current ?? "");
  return (
    <form
      onSubmit={async (e) => {
        e.preventDefault();
        if (!name.trim()) return;
        const ok = await run("work_create_branch", {
          name: name.trim(),
          base: base || null,
          switchTo,
        });
        if (ok) onDone();
      }}
      className="mb-2 space-y-1.5 rounded-lg border border-border bg-surface p-2"
    >
      <input
        // biome-ignore lint/a11y/noAutofocus: opened on purpose by the + button
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder={t("work.branch.namePlaceholder")}
        className={`${inputClass} w-full`}
      />
      <div className="flex items-center gap-1.5 text-xs text-text-muted">
        <span className="shrink-0">{t("work.base")}</span>
        <select
          aria-label={t("work.baseBranch")}
          value={base}
          onChange={(e) => setBase(e.target.value)}
          className={inputClass}
        >
          {!current && <option value="">{t("work.currentHead")}</option>}
          {overview?.branches.map((b) => (
            <option key={b.name} value={b.name}>
              {b.name}
            </option>
          ))}
        </select>
      </div>
      <div className="flex items-center gap-2 text-xs">
        <label className="flex items-center gap-1.5 text-text-muted">
          <input
            type="checkbox"
            checked={switchTo}
            onChange={(e) => setSwitchTo(e.target.checked)}
          />
          {t("work.branch.switchAfter")}
        </label>
        <button
          type="button"
          onClick={onDone}
          className="ml-auto rounded-md px-2 py-1 text-text-muted hover:bg-hover"
        >
          {t("work.cancel")}
        </button>
        <button
          type="submit"
          disabled={busy || !name.trim()}
          className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          {t("work.create")}
        </button>
      </div>
    </form>
  );
}

function folderName(path: string) {
  return (
    path
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() ?? path
  );
}

function Worktrees({ compact }: { compact: boolean }) {
  const t = useT();
  const { overview, busy, run } = useWorkStore();
  const select = useProjectStore((s) => s.select);
  const [adding, setAdding] = useState(false);
  if (!overview) return null;
  const { worktrees } = overview;
  const prunable = worktrees.some((w) => w.prunable);

  const remove = async (wt: Worktree) => {
    const name = folderName(wt.path);
    // A detached HEAD may hold commits no branch has; the backend checks.
    if (wt.changes === 0 && wt.branch) {
      if (
        await confirm(
          t("work.worktree.deleteConfirm", { name, branch: wt.branch }),
          t("work.worktree.deleteTitle"),
        )
      )
        await run("work_remove_worktree", { path: wt.path, force: false });
      return;
    }
    const what = [
      wt.changes
        ? t(
            wt.changes === 1
              ? "work.worktree.uncommittedCount.one"
              : "work.worktree.uncommittedCount",
            { n: wt.changes },
          )
        : wt.changes === null
          ? t("work.worktree.uncommitted")
          : "",
      wt.branch ? "" : t("work.worktree.orphanCommits"),
    ]
      .filter(Boolean)
      .join(t("work.worktree.listJoin"));
    const first = await confirm(
      t("work.worktree.dirtyConfirm", { name, what }),
      t("work.worktree.dirtyTitle"),
      true,
    );
    if (!first) return;
    const second = await confirm(
      t("work.forceDeleteConfirm", { name }),
      t("work.forceDelete"),
      true,
    );
    if (second) await run("work_remove_worktree", { path: wt.path, force: true });
  };

  return (
    <Section
      title="Worktree"
      count={worktrees.length}
      action={
        <>
          {prunable && (
            <button
              type="button"
              disabled={busy}
              onClick={() => void run("work_prune_worktrees", {})}
              title={t("work.worktree.pruneHint")}
              className="mr-1 rounded px-1.5 py-0.5 text-[11px] font-normal text-text-muted hover:bg-hover"
            >
              {t("work.worktree.prune")}
            </button>
          )}
          <IconButton
            label={t("work.worktree.new")}
            disabled={busy || !overview.head.commit}
            onClick={() => setAdding(!adding)}
            className="size-6"
          >
            <Plus size={14} />
          </IconButton>
        </>
      }
    >
      {adding && <NewWorktree onDone={() => setAdding(false)} />}
      <ul className="space-y-px">
        {worktrees.map((wt) => (
          <li
            key={wt.path}
            className={`group flex h-7 items-center gap-2 rounded-md px-2 hover:bg-hover ${
              compact ? "text-xs" : "text-sm"
            }`}
            title={wt.path}
          >
            <FolderTree size={13} className="shrink-0 text-text-subtle" />
            <span
              className={`min-w-0 truncate ${wt.prunable ? "text-text-subtle line-through" : ""}`}
            >
              {wt.isMain ? t("work.worktree.main") : folderName(wt.path)}
            </span>
            {wt.isCurrent && <Badge>{t("work.worktree.open")}</Badge>}
            {wt.managed && <Badge>Claude Code</Badge>}
            {wt.locked && <Badge>{t("work.worktree.locked")}</Badge>}
            {wt.prunable && <Badge>{t("work.worktree.missing")}</Badge>}
            <span className="min-w-0 flex-1 truncate text-[11px] text-text-subtle">
              {wt.branch ?? (wt.commit ? t("work.detached", { commit: wt.commit }) : "")}
            </span>
            {wt.changes !== null && wt.changes > 0 && (
              <span className="shrink-0 text-[11px] text-warning group-hover:hidden">
                {t("work.worktree.changes", { n: wt.changes })}
              </span>
            )}
            {!wt.prunable && (
              <span className="ml-auto hidden shrink-0 items-center gap-0.5 group-hover:flex">
                {!wt.isCurrent && (
                  <button
                    type="button"
                    onClick={() => void select(wt.path)}
                    className="rounded px-1.5 py-0.5 text-[11px] text-accent hover:bg-active"
                  >
                    {t("work.worktree.openAction")}
                  </button>
                )}
                <IconButton
                  label={t("work.worktree.reveal")}
                  onClick={() => void revealItemInDir(wt.path).catch(() => {})}
                  className="size-5"
                >
                  <FolderOpen size={12} />
                </IconButton>
                {!wt.isMain && !wt.isCurrent && (
                  <IconButton
                    label={t("work.deleteNamed", { name: folderName(wt.path) })}
                    disabled={busy}
                    onClick={() => void remove(wt)}
                    className="size-5 hover:text-danger"
                  >
                    <Trash2 size={12} />
                  </IconButton>
                )}
              </span>
            )}
          </li>
        ))}
      </ul>
    </Section>
  );
}

function NewWorktree({ onDone }: { onDone: () => void }) {
  const t = useT();
  const { overview, busy, run } = useWorkStore();
  const [name, setName] = useState("");
  const current = overview?.branches.find((b) => b.name === overview.head.branch)?.name ?? "";
  const [base, setBase] = useState(current);
  const valid = /^[A-Za-z0-9_][A-Za-z0-9._-]{0,63}$/.test(name);
  return (
    <form
      onSubmit={async (e) => {
        e.preventDefault();
        if (!valid) return;
        const ok = await run("work_add_worktree", { name, base: base || null });
        if (ok) onDone();
      }}
      className="mb-2 space-y-1.5 rounded-lg border border-border bg-surface p-2"
    >
      <input
        // biome-ignore lint/a11y/noAutofocus: opened on purpose by the + button
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder={t("work.worktree.namePlaceholder")}
        className={`${inputClass} w-full font-mono`}
      />
      <p className="text-[11px] text-text-subtle">
        {name && !valid
          ? t("work.worktree.invalidName")
          : t("work.worktree.hint", {
              path: `.claude/worktrees/${name || t("work.worktree.nameToken")}`,
              branch: `worktree-${name || t("work.worktree.nameToken")}`,
            })}
      </p>
      <div className="flex items-center gap-1.5 text-xs text-text-muted">
        <span className="shrink-0">{t("work.base")}</span>
        <select
          aria-label={t("work.baseBranch")}
          value={base}
          onChange={(e) => setBase(e.target.value)}
          className={inputClass}
        >
          {!current && <option value="">{t("work.currentHead")}</option>}
          {overview?.branches.map((b) => (
            <option key={b.name} value={b.name}>
              {b.name}
            </option>
          ))}
        </select>
      </div>
      <div className="flex items-center justify-end gap-2 text-xs">
        <button
          type="button"
          onClick={onDone}
          className="rounded-md px-2 py-1 text-text-muted hover:bg-hover"
        >
          {t("work.cancel")}
        </button>
        <button
          type="submit"
          disabled={busy || !valid}
          className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          {t("work.create")}
        </button>
      </div>
    </form>
  );
}
