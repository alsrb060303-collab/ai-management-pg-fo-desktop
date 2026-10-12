import { Cloud, GitMerge, Tag } from "lucide-react";
import { useState } from "react";
import { timeAgo, useT } from "../../i18n";
import { useGitHubStore } from "../../store/github";
import { type Commit, type RefLabel, useWorkStore } from "../../store/work";
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
  const { log, unpushed, hasRemote } = overview;
  if (!log.length) return <p className="text-xs text-text-subtle">{t("work.log.empty")}</p>;
  // The first pushed commit gets the "above isn't on GitHub" line.
  const firstPushed = hasRemote && unpushed > 0 ? log.findIndex((c) => c.pushed) : -1;

  return (
    <div>
      <ol>
        {log.map((c, i) => (
          <li key={c.hash}>
            {i === firstPushed && i > 0 && (
              <div className="my-1 flex items-center gap-2 pl-1 text-[10.5px] font-medium text-accent">
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
              last={i === log.length - 1}
              unpushed={hasRemote && !c.pushed}
              url={view ? `${view.url}/commit/${c.hash}` : null}
            />
          </li>
        ))}
      </ol>
      <p className="mt-2 text-[11px] text-text-subtle">{t("work.log.hint")}</p>
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
}: {
  commit: Commit;
  compact: boolean;
  now: boolean;
  last: boolean;
  unpushed: boolean;
  url: string | null;
}) {
  const t = useT();
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
          <span className="min-w-0 truncate" title={c.subject}>
            {c.subject}
          </span>
          {now && (
            <span className="shrink-0 rounded-full bg-primary px-1.5 py-px text-[10px] font-medium text-primary-fg">
              {t("work.log.now")}
            </span>
          )}
        </div>
        <div className="mt-0.5 flex flex-wrap items-center gap-1 text-[11px] text-text-subtle">
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
            <span className="shrink-0 rounded-full bg-accent-soft px-1.5 text-[10px] text-accent">
              {t("work.log.notPushed")}
            </span>
          )}
          {c.refs.map((r) => (
            <RefChip key={`${r.kind}:${r.name}`} label={r} />
          ))}
        </div>
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
      className={`flex max-w-40 shrink-0 items-center gap-0.5 truncate rounded-full border px-1.5 text-[10px] ${style}`}
    >
      {label.kind === "remote" && <Cloud size={10} className="shrink-0" />}
      {label.kind === "tag" && <Tag size={10} className="shrink-0" />}
      <span className="truncate">{label.name}</span>
    </span>
  );
}
