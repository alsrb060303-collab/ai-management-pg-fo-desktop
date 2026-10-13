import { ChevronRight, Cloud, GitMerge, Tag } from "lucide-react";
import { useState } from "react";
import { timeAgo, useT } from "../../i18n";
import { useGitHubStore } from "../../store/github";
import { type Commit, type RefLabel, useWorkStore } from "../../store/work";
import { DiffView } from "./DiffView";
import { open } from "./GitHubSection";

/**
 * The checked-out branch's recent commits as a timeline: hollow dots are not
 * on GitHub yet, filled ones are, with a line where the two meet.
 */
export function History({ compact }: { compact: boolean }) {
  const t = useT();
  const overview = useWorkStore((s) => s.overview);
  const view = useGitHubStore((s) => s.view);
  if (!overview) return null;
  return (
    <Timeline
      commits={overview.log}
      unpushed={overview.hasRemote ? overview.unpushed : 0}
      compact={compact}
      urlOf={view ? (c) => `${view.url}/commit/${c.hash}` : () => null}
      expandable
      hint={t("work.log.hint")}
    />
  );
}

/** A list of commits, newest first, drawn as a timeline. */
export function Timeline({
  commits,
  unpushed,
  compact,
  urlOf,
  expandable,
  hint,
}: {
  commits: Commit[];
  /** Commits not on GitHub yet (0 when there is no remote). */
  unpushed: number;
  compact: boolean;
  urlOf: (c: Commit) => string | null;
  /** Clicking a commit shows its changes (local repositories only). */
  expandable: boolean;
  hint?: string;
}) {
  const t = useT();
  if (!commits.length) return <p className="text-xs text-text-subtle">{t("work.log.empty")}</p>;
  // The first pushed commit gets the "above isn't on GitHub" line.
  const firstPushed = unpushed > 0 ? commits.findIndex((c) => c.pushed) : -1;

  return (
    <div>
      <ol>
        {commits.map((c, i) => (
          <li key={c.hash}>
            {i === firstPushed && i > 0 && (
              <div className="my-1 flex items-center gap-2 pl-1 text-fs-10.5 font-medium text-accent">
                <span className="h-px flex-1 border-t border-dashed border-accent/60" />
                {t(unpushed === 1 ? "work.log.unpushed.one" : "work.log.unpushed", {
                  n: unpushed,
                })}
                <span className="h-px w-3 border-t border-dashed border-accent/60" />
              </div>
            )}
            <CommitRow
              commit={c}
              compact={compact}
              now={i === 0}
              last={i === commits.length - 1}
              unpushed={unpushed > 0 && !c.pushed}
              url={urlOf(c)}
              expandable={expandable}
            />
          </li>
        ))}
      </ol>
      {hint && <p className="mt-2 text-fs-11 text-text-subtle">{hint}</p>}
    </div>
  );
}

function CommitRow({
  commit: c,
  compact,
  now,
  last,
  unpushed,
  url,
  expandable,
}: {
  commit: Commit;
  compact: boolean;
  now: boolean;
  last: boolean;
  unpushed: boolean;
  url: string | null;
  expandable: boolean;
}) {
  const t = useT();
  const { openCommit, commitDiff, showCommit } = useWorkStore();
  const open_ = expandable && openCommit === c.hash;
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard
      .writeText(c.hash)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      })
      .catch(() => {});
  };
  const dot = c.merge
    ? "border-merged bg-merged/15 text-merged"
    : unpushed
      ? "border-accent bg-surface"
      : "border-success bg-success";

  return (
    <div className="group flex gap-2">
      {/* Rail */}
      <div className="relative flex w-4 shrink-0 justify-center">
        <span
          className={`absolute w-0.5 ${now ? "top-2.5" : "top-0"} ${last ? "h-2.5" : "bottom-0"} ${
            unpushed ? "bg-accent/40" : "bg-success/40"
          }`}
        />
        <span
          className={`relative z-10 mt-1.5 flex items-center justify-center rounded-full border-2 ${
            c.merge ? "size-4" : "size-3"
          } ${dot}`}
        >
          {c.merge && <GitMerge size={9} strokeWidth={3} />}
        </span>
      </div>
      {/* Body */}
      <div className={`min-w-0 flex-1 pb-2 ${compact ? "text-xs" : "text-sm"}`}>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => (expandable ? void showCommit(c.hash) : url && open(url))}
            aria-expanded={expandable ? open_ : undefined}
            title={`${c.subject}\n${expandable ? t(open_ ? "work.log.hideChanges" : "work.log.showChanges") : t("work.log.open")}`}
            className="flex min-w-0 items-center gap-1 rounded text-left hover:text-accent"
          >
            {expandable && (
              <ChevronRight
                size={12}
                className={`shrink-0 text-text-subtle transition-transform ${open_ ? "rotate-90" : ""}`}
              />
            )}
            <span className="min-w-0 truncate">{c.subject}</span>
          </button>
          {now && (
            <span className="shrink-0 rounded-full bg-primary px-1.5 py-px text-fs-10 font-medium text-primary-fg">
              {t("work.log.now")}
            </span>
          )}
        </div>
        <div className="mt-0.5 flex flex-wrap items-center gap-1 text-fs-11 text-text-subtle">
          <button
            type="button"
            onClick={() => (url ? open(url) : copy())}
            onContextMenu={(e) => {
              e.preventDefault();
              copy();
            }}
            title={`${c.hash}\n${url ? t("work.log.open") : t("work.log.copy")}`}
            className="rounded font-mono hover:text-accent hover:underline"
          >
            {copied ? t("work.log.copied") : c.short}
          </button>
          <span>·</span>
          <span className="truncate">{c.author}</span>
          <span>·</span>
          <span className="shrink-0">{timeAgo(c.time)}</span>
          {unpushed && (
            <span className="shrink-0 rounded-full bg-accent-soft px-1.5 text-fs-10 text-accent">
              {t("work.log.notPushed")}
            </span>
          )}
          {c.refs.map((r) => (
            <RefChip key={`${r.kind}:${r.name}`} label={r} />
          ))}
        </div>
        {open_ && <DiffView diff={commitDiff} compact={compact} />}
      </div>
    </div>
  );
}

function RefChip({ label }: { label: RefLabel }) {
  const style =
    label.kind === "local"
      ? "border-accent/40 text-accent"
      : label.kind === "remote"
        ? "border-border-strong text-text-muted"
        : "border-warning/50 text-warning";
  return (
    <span
      className={`flex max-w-40 shrink-0 items-center gap-0.5 truncate rounded-full border px-1.5 text-fs-10 ${style}`}
    >
      {label.kind === "remote" && <Cloud size={10} className="shrink-0" />}
      {label.kind === "tag" && <Tag size={10} className="shrink-0" />}
      <span className="truncate">{label.name}</span>
    </span>
  );
}
