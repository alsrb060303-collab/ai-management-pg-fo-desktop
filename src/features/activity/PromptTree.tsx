import { ChevronRight, FileText, GitCommitHorizontal, Upload } from "lucide-react";
import { useMemo, useState } from "react";
import { timeAgo, useT } from "../../i18n";
import { type PromptEvent, useActivityStore } from "../../store/activity";
import { TOOLS } from "../../tools";
import { layoutSessions } from "./layout";
import { Rail } from "./Rail";

const DOT: Record<string, string> = { claude: "bg-accent", codex: "bg-info" };
const LABEL: Record<string, string> = { claude: "text-accent", codex: "text-info" };

/** The prompt tree: one lane per session, a dot per prompt. */
export function PromptTree({ compact }: { compact: boolean }) {
  const t = useT();
  const log = useActivityStore((s) => s.prompts);
  const more = useActivityStore((s) => s.morePrompts);
  const loadingMore = useActivityStore((s) => s.loadingMore === "prompts");
  const loadMore = useActivityStore((s) => s.loadMore);

  const events = useMemo(() => [...(log?.events ?? [])].sort((a, b) => b.at - a.at), [log]);
  const layout = useMemo(
    () => layoutSessions(events.map((e) => ({ session: `${e.tool}:${e.session}`, tone: e.tool }))),
    [events],
  );

  if (!log) return null;
  if (!events.length)
    return <p className="text-xs text-text-subtle">{t("activity.prompts.empty")}</p>;

  return (
    <div>
      <p className={`mb-2 text-text-subtle ${compact ? "text-fs-11" : "text-xs"}`}>
        {t("activity.prompts.legend")}
      </p>
      <ol>
        {events.map((e, i) => (
          <li key={e.id} className="flex gap-2">
            <Rail
              row={layout.gutter[i]}
              lanes={layout.lanes}
              compact={compact}
              dot={<span className={`size-3 rounded-full ${DOT[e.tool] ?? "bg-text-muted"}`} />}
            />
            <PromptRow event={e} compact={compact} />
          </li>
        ))}
      </ol>
      <div className="mt-2 text-center text-xs">
        {more ? (
          <button
            type="button"
            disabled={loadingMore}
            onClick={() => void loadMore("prompts")}
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

function PromptRow({ event: e, compact }: { event: PromptEvent; compact: boolean }) {
  const t = useT();
  const showCommit = useActivityStore((s) => s.showCommit);
  const [open, setOpen] = useState(false);
  const [filesOpen, setFilesOpen] = useState(false);
  const text = compact ? "text-xs" : "text-sm";
  const long = e.text.length > 100 || e.text.includes("\n");
  const toolName = TOOLS[e.tool]?.name ?? e.tool;

  return (
    <div className={`min-w-0 flex-1 pb-2.5 ${text}`}>
      <div className="flex items-start gap-1">
        <button
          type="button"
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          aria-label={t(open ? "activity.collapse" : "activity.expand")}
          disabled={!long}
          className="mt-0.5 shrink-0 rounded text-text-subtle enabled:hover:text-accent"
        >
          <ChevronRight
            size={12}
            className={`transition-transform ${open ? "rotate-90" : ""} ${long ? "" : "opacity-30"}`}
          />
        </button>
        <p
          className={`selectable min-w-0 flex-1 break-words whitespace-pre-wrap ${
            open ? "" : compact ? "line-clamp-2" : "line-clamp-3"
          }`}
        >
          {e.text}
        </p>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1 pl-4 text-fs-11 text-text-subtle">
        <span className={`font-medium ${LABEL[e.tool] ?? ""}`}>{toolName}</span>
        <span>·</span>
        <span className="max-w-32 truncate">{e.branch ?? t("activity.prompts.noBranch")}</span>
        <span>·</span>
        <span className="shrink-0">{timeAgo(e.at / 1000)}</span>
        {e.files.length > 0 && (
          <button
            type="button"
            onClick={() => setFilesOpen(!filesOpen)}
            aria-expanded={filesOpen}
            className="flex shrink-0 items-center gap-0.5 rounded-full border border-border bg-surface px-1.5 py-px text-fs-10 hover:border-border-strong"
          >
            <FileText size={10} />
            {t("activity.prompts.files", { n: e.files.length })}
          </button>
        )}
        {e.commits.map((sha) => (
          <button
            key={sha}
            type="button"
            onClick={() => showCommit(sha)}
            title={t("activity.prompts.commitHint")}
            className="flex shrink-0 items-center gap-0.5 rounded-full border border-border bg-surface px-1.5 py-px font-mono text-fs-10 hover:border-accent hover:text-accent"
          >
            <GitCommitHorizontal size={10} />
            {sha.slice(0, 7)}
          </button>
        ))}
        {e.pushed && (
          <span className="flex shrink-0 items-center gap-0.5 rounded-full border border-success/50 bg-surface px-1.5 py-px text-fs-10 text-success">
            <Upload size={10} />
            {t("activity.prompts.push")}
          </span>
        )}
      </div>
      {filesOpen && (
        <ul className="selectable mt-1 ml-4 space-y-0.5 rounded-lg bg-sunken px-3 py-2 font-mono text-fs-11 text-text-muted">
          {e.files.map((f) => (
            <li key={f} className="break-all">
              {f}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
