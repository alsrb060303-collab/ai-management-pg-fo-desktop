import { useEffect } from "react";
import { Segmented } from "../../components/ui";
import { useT } from "../../i18n";
import { listenActivityChanges, useActivityStore } from "../../store/activity";
import { useGitHubStore } from "../../store/github";
import { useProjectStore } from "../../store/project";
import { LoadingSpin } from "../work/FlowCard";
import { GitTree } from "./GitTree";
import { PromptTree } from "./PromptTree";

/** How often the sources are re-read while the trees are on screen. */
const REFRESH_MS = 60_000;

/**
 * The open project's work trees: what went to GitHub, and the prompts sent to
 * AI tools. Shared by the sidebar (`compact`) and the main window's page.
 */
export function WorkTrees({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const project = useProjectStore((s) => s.current);
  const choose = useProjectStore((s) => s.choose);
  const signedIn = useGitHubStore((s) => !!s.status?.account);
  const { view, setView, load, refresh, reload, git, prompts, loading, refreshing, error } =
    useActivityStore();

  useEffect(() => {
    listenActivityChanges();
  }, []);

  useEffect(() => {
    void load(project);
  }, [project, load]);

  // Refresh when shown, on window focus and every minute while visible.
  useEffect(() => {
    if (!project) return;
    const run = () => {
      if (document.visibilityState === "visible") {
        void useActivityStore.getState().refresh(useGitHubStore.getState().status?.account != null);
      }
    };
    run();
    const timer = setInterval(run, REFRESH_MS);
    window.addEventListener("focus", run);
    document.addEventListener("visibilitychange", run);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", run);
      document.removeEventListener("visibilitychange", run);
    };
  }, [project]);

  // Signing in to GitHub adds PR and CI events.
  useEffect(() => {
    if (project && signedIn) void refresh(true);
  }, [project, signedIn, refresh]);

  if (!project) {
    return (
      <div className="space-y-3 text-sm">
        <p className="text-text-muted">{t("activity.noProject")}</p>
        <button
          type="button"
          onClick={() => void choose()}
          className="rounded-lg border border-border bg-surface-raised px-3 py-1.5 text-sm hover:border-border-strong"
        >
          {t("activity.chooseFolder")}
        </button>
      </div>
    );
  }

  const empty = !git && !prompts;
  return (
    <div className={compact ? "space-y-3" : "space-y-5"}>
      <div className="flex items-center gap-3">
        <Segmented
          label={t("activity.view")}
          value={view}
          options={[
            { value: "git", label: t("activity.view.git") },
            { value: "prompts", label: t("activity.view.prompts") },
          ]}
          onChange={setView}
        />
        {refreshing && !loading && (
          <span role="status" className="text-fs-11 text-text-subtle">
            {t("activity.refreshing")}
          </span>
        )}
      </div>
      {error && (
        <div className="flex items-start gap-2 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">
          <p className="selectable min-w-0 flex-1 break-words whitespace-pre-wrap">
            {t("activity.error", { error })}
          </p>
          <button
            type="button"
            onClick={() => void (empty ? load(project) : reload())}
            className="shrink-0 rounded underline"
          >
            {t("activity.retry")}
          </button>
        </div>
      )}
      {loading && empty && <LoadingSpin label={t("activity.loading")} />}
      {view === "git" ? <GitTree compact={compact} /> : <PromptTree compact={compact} />}
    </div>
  );
}
