import {
  Check,
  ChevronRight,
  Clock,
  ExternalLink,
  GitBranch,
  GitMerge,
  GitPullRequest,
  GitPullRequestClosed,
  type LucideIcon,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { intlLocale, type MessageKey, timeAgo, useT } from "../../i18n";
import { type GitEvent, useActivityStore } from "../../store/activity";
import { useGitHubStore } from "../../store/github";
import { open as openUrl } from "../work/GitHubSection";
import { type GraphRow, layoutGraph, reachableFrom } from "./layout";
import { Rail } from "./Rail";

type Ci = "passed" | "failed" | "pending";

function ciOf(state: string | null): Ci {
  if (state && /fail|error|cancel|timed|neutral/i.test(state)) return "failed";
  if (state && /success|pass|complete/i.test(state)) return "passed";
  return "pending";
}

interface Row {
  id: string;
  /** A commit (dot on its branch's lane) or a stand-alone event. */
  commit: boolean;
  event: GitEvent;
  badges: GitEvent[];
  pushed: boolean;
}

/** Builds the rows, newest first: commits with their push/PR/CI events attached. */
function buildRows(events: GitEvent[], tips: (string | null)[]): Row[] {
  const sorted = [...events].sort((a, b) => b.at - a.at);
  const commits = sorted.filter((e) => (e.kind === "commit" || e.kind === "merge") && e.sha);
  const seen = new Set<string>();
  const unique = commits.filter((e) => {
    const sha = e.sha as string;
    if (seen.has(sha)) return false;
    seen.add(sha);
    return true;
  });
  const parentsOf = new Map(unique.map((e) => [e.sha as string, e.parents]));
  const pushTips = sorted.filter((e) => e.kind === "push").map((e) => e.sha);
  const pushed = reachableFrom([...tips, ...pushTips], parentsOf);

  const rows = new Map<string, Row>();
  for (const e of unique) {
    rows.set(e.sha as string, {
      id: e.id,
      commit: true,
      event: e,
      badges: [],
      pushed: pushed.has(e.sha as string),
    });
  }
  const standalone: Row[] = [];
  for (const e of sorted) {
    if (e.kind === "commit" || e.kind === "merge") continue;
    const target = e.sha
      ? (rows.get(e.sha) ??
        [...rows.values()].find((r) => (r.event.sha as string).startsWith(e.sha as string)))
      : undefined;
    if (target && e.kind !== "branch") target.badges.push(e);
    else if (e.kind !== "push" && e.kind !== "ci") {
      standalone.push({ id: e.id, commit: false, event: e, badges: [], pushed: true });
    }
  }
  return [...rows.values(), ...standalone].sort((a, b) => b.event.at - a.event.at);
}

/** The GitHub tree: commits on branch lanes, with pushes, PRs and CI beside them. */
export function GitTree({ compact }: { compact: boolean }) {
  const t = useT();
  const log = useActivityStore((s) => s.git);
  const more = useActivityStore((s) => s.moreGit);
  const loadingMore = useActivityStore((s) => s.loadingMore === "git");
  const loadMore = useActivityStore((s) => s.loadMore);
  const focusSha = useActivityStore((s) => s.focusSha);
  const clearFocus = useActivityStore((s) => s.clearFocus);
  const repoUrl = useGitHubStore((s) => s.view?.url);

  const [expanded, setExpanded] = useState<string | null>(null);
  const [highlight, setHighlight] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);
  const refs = useRef(new Map<string, HTMLElement>());

  const trunk = log?.branches.find((b) => b.isDefault)?.name ?? null;
  const rows = useMemo(
    () =>
      log
        ? buildRows(
            log.events,
            log.branches.map((b) => b.remote),
          )
        : [],
    [log],
  );
  const layout = useMemo(() => {
    const graph: GraphRow[] = rows.map((r) => ({
      sha: r.commit ? r.event.sha : null,
      parents: r.event.parents,
      group: r.event.branch ?? trunk ?? "",
      tone: r.pushed ? "pushed" : "local",
    }));
    return layoutGraph(graph, trunk);
  }, [rows, trunk]);

  // A commit chip in the prompt tree sends us here: find it, load older pages if needed.
  useEffect(() => {
    if (!focusSha) return;
    const row = rows.find((r) => r.commit && (r.event.sha as string).startsWith(focusSha));
    if (row) {
      setMissing(false);
      setExpanded(row.id);
      setHighlight(row.id);
      clearFocus();
      requestAnimationFrame(() =>
        refs.current.get(row.id)?.scrollIntoView({ block: "center", behavior: "smooth" }),
      );
      return;
    }
    if (loadingMore || useActivityStore.getState().loadingMore) return;
    const giveUp = () => {
      setMissing(true);
      clearFocus();
    };
    if (!more) {
      giveUp();
      return;
    }
    // One page per pass; a failed or empty page ends the search (no retry loop).
    void loadMore("git").then((added) => {
      if (!added && useActivityStore.getState().focusSha === focusSha) giveUp();
    });
  }, [focusSha, rows, more, loadingMore, loadMore, clearFocus]);

  useEffect(() => {
    if (!highlight) return;
    const timer = setTimeout(() => setHighlight(null), 2500);
    return () => clearTimeout(timer);
  }, [highlight]);

  if (!log) return null;
  if (!rows.length) return <p className="text-xs text-text-subtle">{t("activity.git.empty")}</p>;

  return (
    <div>
      <p className={`mb-2 text-text-subtle ${compact ? "text-fs-11" : "text-xs"}`}>
        {t("activity.git.legend")}
      </p>
      {missing && (
        <p role="status" className="mb-2 rounded-lg bg-accent-soft px-3 py-1.5 text-xs text-accent">
          {t("activity.git.focusMissing")}
        </p>
      )}
      <ol>
        {rows.map((row, i) => (
          <li
            key={row.id}
            ref={(el) => {
              if (el) refs.current.set(row.id, el);
              else refs.current.delete(row.id);
            }}
            className={`flex gap-2 rounded-md ${highlight === row.id ? "bg-accent-soft" : ""}`}
          >
            <Rail
              row={layout.gutter[i]}
              lanes={layout.lanes}
              compact={compact}
              dot={<Dot row={row} />}
            />
            <RowBody
              row={row}
              compact={compact}
              open={expanded === row.id}
              onToggle={() => setExpanded(expanded === row.id ? null : row.id)}
              commitUrl={(sha) => (repoUrl ? `${repoUrl}/commit/${sha}` : null)}
            />
          </li>
        ))}
      </ol>
      <div className="mt-2 text-center text-xs">
        {more ? (
          <button
            type="button"
            disabled={loadingMore}
            onClick={() => void loadMore("git")}
            className="rounded-lg border border-border bg-surface-raised px-3 py-1.5 hover:border-border-strong disabled:opacity-60"
          >
            {loadingMore ? t("activity.loadingMore") : t("activity.more")}
          </button>
        ) : (
          <span className="text-text-subtle">{t("activity.end")}</span>
        )}
      </div>
    </div>
  );
}

const PR_ICON: Record<string, LucideIcon> = {
  prOpened: GitPullRequest,
  prMerged: GitMerge,
  prClosed: GitPullRequestClosed,
};
const PR_TONE: Record<string, string> = {
  prOpened: "text-success",
  prMerged: "text-merged",
  prClosed: "text-danger",
};
const PR_LABEL: Record<string, MessageKey> = {
  prOpened: "activity.git.pr.opened",
  prMerged: "activity.git.pr.merged",
  prClosed: "activity.git.pr.closed",
};

function Dot({ row }: { row: Row }) {
  const e = row.event;
  if (row.commit) {
    if (e.kind === "merge" || e.parents.length > 1) {
      return (
        <span className="flex size-4 items-center justify-center rounded-full border-2 border-merged bg-merged/15 text-merged">
          <GitMerge size={9} strokeWidth={3} />
        </span>
      );
    }
    return (
      <span
        className={`size-3 rounded-full border-2 ${
          row.pushed ? "border-success bg-success" : "border-accent bg-surface"
        }`}
      />
    );
  }
  const Icon = PR_ICON[e.kind] ?? GitBranch;
  return (
    <span
      className={`flex size-4 items-center justify-center rounded-full border border-border bg-surface ${PR_TONE[e.kind] ?? "text-text-muted"}`}
    >
      <Icon size={10} />
    </span>
  );
}

function RowBody({
  row,
  compact,
  open,
  onToggle,
  commitUrl,
}: {
  row: Row;
  compact: boolean;
  open: boolean;
  onToggle: () => void;
  commitUrl: (sha: string) => string | null;
}) {
  const t = useT();
  const e = row.event;
  const [copied, setCopied] = useState(false);
  const text = compact ? "text-xs" : "text-sm";
  const sha = e.sha ?? "";
  const url = e.url ?? (sha ? commitUrl(sha) : null);

  const copy = () => {
    void navigator.clipboard
      .writeText(sha)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      })
      .catch(() => {});
  };

  if (!row.commit) {
    const label =
      e.kind === "branch"
        ? t("activity.git.branchCreated", { name: e.branch ?? "" })
        : `${t(PR_LABEL[e.kind] ?? "activity.git.push", { n: e.pr ?? 0 })}${e.title ? ` · ${e.title}` : ""}`;
    return (
      <div className={`min-w-0 flex-1 pb-2 ${text}`}>
        <div className="flex min-w-0 items-center gap-1.5 pt-px">
          {url ? (
            <button
              type="button"
              onClick={() => openUrl(url)}
              title={t("activity.git.openOnGitHub")}
              className="min-w-0 truncate rounded text-left text-text-muted hover:text-accent"
            >
              {label}
            </button>
          ) : (
            <span className="min-w-0 truncate text-text-muted">{label}</span>
          )}
          <span className="shrink-0 text-fs-11 text-text-subtle">{timeAgo(e.at / 1000)}</span>
        </div>
      </div>
    );
  }

  const isMerge = e.kind === "merge" || e.parents.length > 1;
  return (
    <div className={`min-w-0 flex-1 pb-2 ${text}`}>
      <div className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={open}
          title={`${e.title ?? sha}\n${t(open ? "activity.collapse" : "activity.expand")}`}
          className="flex min-w-0 items-center gap-1 rounded text-left hover:text-accent"
        >
          <ChevronRight
            size={12}
            className={`shrink-0 text-text-subtle transition-transform ${open ? "rotate-90" : ""}`}
          />
          <span className="min-w-0 truncate">{e.title || sha.slice(0, 7)}</span>
        </button>
        {row.badges.map((b) => (
          <Badge key={b.id} event={b} />
        ))}
      </div>
      <div className="mt-0.5 flex flex-wrap items-center gap-1 pl-4 text-fs-11 text-text-subtle">
        <span className="font-mono">{sha.slice(0, 7)}</span>
        {e.branch && (
          <>
            <span>·</span>
            <span className="max-w-32 truncate">{e.branch}</span>
          </>
        )}
        <span>·</span>
        <span className="shrink-0">{timeAgo(e.at / 1000)}</span>
        {isMerge && (
          <span className="shrink-0 rounded-full bg-sunken px-1.5 text-fs-10 text-merged">
            {t("activity.git.merge")}
          </span>
        )}
        {!row.pushed && (
          <span className="shrink-0 rounded-full bg-accent-soft px-1.5 text-fs-10 text-accent">
            {t("activity.git.notPushed")}
          </span>
        )}
      </div>
      {open && (
        <dl className="selectable mt-1.5 ml-4 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 rounded-lg bg-sunken px-3 py-2 text-fs-11">
          <Detail label={t("activity.git.sha")}>
            <span className="font-mono break-all">{sha}</span>
          </Detail>
          {e.author && <Detail label={t("activity.git.author")}>{e.author}</Detail>}
          <Detail label={t("activity.git.time")}>
            {new Date(e.at).toLocaleString(intlLocale())}
          </Detail>
          {e.branch && <Detail label={t("activity.git.branch")}>{e.branch}</Detail>}
          <dd className="col-span-2 mt-0.5 flex flex-wrap gap-2">
            <button
              type="button"
              onClick={copy}
              className="rounded border border-border bg-surface-raised px-2 py-0.5 hover:border-border-strong"
            >
              {copied ? t("activity.git.copied") : t("activity.git.copySha")}
            </button>
            {url && (
              <button
                type="button"
                onClick={() => openUrl(url)}
                className="flex items-center gap-1 rounded border border-border bg-surface-raised px-2 py-0.5 hover:border-border-strong"
              >
                <ExternalLink size={11} />
                {t("activity.git.openOnGitHub")}
              </button>
            )}
          </dd>
        </dl>
      )}
    </div>
  );
}

function Detail({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-text-subtle">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </>
  );
}

/** A small pill for a PR event, a push or a CI result on a commit. */
function Badge({ event: e }: { event: GitEvent }) {
  const t = useT();
  let Icon: LucideIcon;
  let tone: string;
  let label: string;
  let short: string | null = null;
  if (e.kind === "ci") {
    const ci = ciOf(e.state);
    Icon = ci === "passed" ? Check : ci === "failed" ? X : Clock;
    tone = ci === "passed" ? "text-success" : ci === "failed" ? "text-danger" : "text-warning";
    label = t(`activity.git.ci.${ci}`);
  } else if (e.kind === "push") {
    Icon = GitBranch;
    tone = "text-text-muted";
    label = t("activity.git.pushOnly", { name: e.branch ?? "" });
    short = t("activity.git.push");
  } else {
    Icon = PR_ICON[e.kind] ?? GitPullRequest;
    tone = PR_TONE[e.kind] ?? "text-text-muted";
    label = t(PR_LABEL[e.kind] ?? "activity.git.pr.opened", { n: e.pr ?? 0 });
    short = e.pr ? `#${e.pr}` : null;
  }
  const body = (
    <>
      <Icon size={10} strokeWidth={e.kind === "ci" ? 3 : 2} className="shrink-0" />
      {short && <span>{short}</span>}
    </>
  );
  const cls = `flex shrink-0 items-center gap-0.5 rounded-full border border-border bg-surface px-1.5 py-px text-fs-10 ${tone}`;
  return e.url ? (
    <button
      type="button"
      onClick={() => e.url && openUrl(e.url)}
      title={label}
      aria-label={label}
      className={`${cls} hover:border-border-strong`}
    >
      {body}
    </button>
  ) : (
    <span title={label} role="img" aria-label={label} className={cls}>
      {body}
    </span>
  );
}
