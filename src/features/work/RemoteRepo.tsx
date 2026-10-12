import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ExternalLink,
  FolderDown,
  GitBranch,
  GitCommitHorizontal,
  GitPullRequest,
  Lock,
  Plus,
  RefreshCw,
  Search,
  Shield,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { timeAgo, useT } from "../../i18n";
import { useGitHubStore } from "../../store/github";
import { useProjectStore } from "../../store/project";
import { type RemoteView, type RepoSummary, useRemoteStore } from "../../store/remote";
import {
  ChecksSummary,
  Collapsible,
  IssueList,
  inputClass,
  open,
  PullBadge,
  PullList,
} from "./GitHubSection";
import { RemoteFiles } from "./RemoteFiles";

/** Repositories listed before the search narrows them down. */
const LIST_LIMIT = 40;

const seconds = (iso: string | null | undefined) => (iso ? Date.parse(iso) / 1000 : 0);

/**
 * "My GitHub repositories": pick one to open its local clone as the project,
 * or, without a clone, to look at it on GitHub (and clone it from there).
 */
export function RepoPicker({ compact }: { compact: boolean }) {
  const t = useT();
  const account = useGitHubStore((s) => s.status?.account);
  const current = useProjectStore((s) => s.current);
  const recent = useProjectStore((s) => s.recent);
  const select = useProjectStore((s) => s.select);
  const {
    repos,
    reposLoading,
    clones,
    slug,
    loadRepos,
    findClones,
    open: openRemote,
    close,
  } = useRemoteStore();
  const [shown, setShown] = useState(false);
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (shown && account) void loadRepos();
  }, [shown, account, loadRepos]);

  // Which repositories already have a clone among the recent projects.
  useEffect(() => {
    if (shown) void findClones(recent).catch(() => {});
  }, [shown, recent, findClones]);

  if (!account) return null;

  const q = query.trim().toLowerCase();
  const matches = (repos ?? []).filter(
    (r) => !q || r.slug.toLowerCase().includes(q) || r.description?.toLowerCase().includes(q),
  );

  const pick = (repo: RepoSummary) => {
    const local = clones[repo.slug.toLowerCase()];
    if (local) {
      close();
      void select(local);
    } else {
      openRemote(repo.slug);
    }
    setShown(false);
    setQuery("");
  };

  return (
    <section className={`space-y-1.5 ${compact ? "text-xs" : "text-sm"}`}>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => setShown(!shown)}
          className="flex min-w-0 items-center gap-1.5 rounded-md border border-border bg-surface-raised px-2.5 py-1 text-xs hover:border-border-strong"
        >
          <GitBranch size={13} className="shrink-0 text-text-subtle" />
          <span className="truncate">{slug ?? t("work.repos.title")}</span>
        </button>
        {shown && (
          <button
            type="button"
            onClick={() => void loadRepos(true)}
            title={t("work.refresh")}
            className="rounded p-1 text-text-subtle hover:bg-hover hover:text-text"
          >
            <RefreshCw size={12} className={reposLoading ? "animate-spin" : ""} />
          </button>
        )}
      </div>
      {shown && (
        <div className="space-y-1.5 rounded-lg border border-border bg-surface-raised p-2">
          <div className="flex items-center gap-1.5">
            <Search size={12} className="shrink-0 text-text-subtle" />
            <input
              // biome-ignore lint/a11y/noAutofocus: opened on purpose
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("work.repos.search")}
              className={inputClass}
            />
          </div>
          {reposLoading && !repos ? (
            <p className="px-1 text-xs text-text-subtle">{t("work.loading")}</p>
          ) : matches.length === 0 ? (
            <p className="px-1 text-xs text-text-subtle">{t("work.repos.none")}</p>
          ) : (
            <ul className="max-h-72 space-y-px overflow-y-auto">
              {matches.slice(0, q ? 200 : LIST_LIMIT).map((r) => {
                const local = clones[r.slug.toLowerCase()];
                const active = local ? local === current && !slug : r.slug === slug;
                return (
                  <li key={r.slug}>
                    <button
                      type="button"
                      onClick={() => pick(r)}
                      title={local ?? r.description ?? r.slug}
                      className={`flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs hover:bg-hover ${active ? "bg-hover" : ""}`}
                    >
                      <span className="min-w-0 flex-1 truncate">{r.slug}</span>
                      {r.private && <Lock size={11} className="shrink-0 text-text-subtle" />}
                      <span
                        className={`shrink-0 rounded-full px-1.5 py-px text-[10px] ${local ? "bg-success/15 text-success" : "bg-sunken text-text-muted"}`}
                      >
                        {t(local ? "work.repos.local" : "work.repos.remote")}
                      </span>
                      {!compact && (
                        <span className="w-16 shrink-0 text-right text-[11px] text-text-subtle">
                          {timeAgo(seconds(r.pushedAt))}
                        </span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}

/** A repository seen on GitHub only: branches, commits, PRs, issues, and cloning it. */
export function RemoteRepo({ compact }: { compact: boolean }) {
  const t = useT();
  const { slug, data, loading, busy, error, close, load, dismissError } = useRemoteStore();
  if (!slug) return null;

  return (
    <div className={`space-y-4 ${compact ? "text-xs" : "text-sm"}`}>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={close}
          title={t("work.remote.back")}
          className="rounded p-1 text-text-subtle hover:bg-hover hover:text-text"
        >
          <ArrowLeft size={14} />
        </button>
        <button
          type="button"
          onClick={() => open(`https://github.com/${slug}`)}
          className="flex min-w-0 items-center gap-1 truncate font-semibold hover:underline"
          title={t("work.github.openOnGitHub")}
        >
          {slug}
          <ExternalLink size={12} className="shrink-0 text-text-subtle" />
        </button>
        <button
          type="button"
          onClick={() => void load()}
          title={t("work.refresh")}
          className="ml-auto rounded p-1 text-text-subtle hover:bg-hover hover:text-text"
        >
          <RefreshCw size={13} className={loading || busy ? "animate-spin" : ""} />
        </button>
      </div>
      {error && (
        <div className="flex items-start gap-2 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">
          <p className="selectable min-w-0 flex-1 whitespace-pre-wrap break-words">{error}</p>
          <button type="button" aria-label={t("work.close")} onClick={dismissError}>
            <X size={13} />
          </button>
        </div>
      )}
      <CloneCard />
      {!data ? (
        loading && <p className="text-xs text-text-muted">{t("work.loading")}</p>
      ) : (
        <>
          <Branches data={data} compact={compact} />
          <BranchDetail data={data} compact={compact} />
          <RemoteFiles compact={compact} />
          <Commits data={data} />
          <PullList pulls={data.view.pulls} />
          <IssueList issues={data.view.issues} />
        </>
      )}
    </div>
  );
}

function CloneCard() {
  const t = useT();
  const { busy, clone, close } = useRemoteStore();
  const select = useProjectStore((s) => s.select);
  const [cloning, setCloning] = useState(false);

  const start = async () => {
    const parent = await openDialog({ directory: true, title: t("work.remote.cloneWhere") });
    if (typeof parent !== "string") return;
    setCloning(true);
    const path = await clone(parent);
    setCloning(false);
    if (path) {
      close();
      await select(path);
    }
  };

  return (
    <div className="space-y-2 rounded-lg border border-border bg-surface-raised p-3">
      <p className="text-text-muted">{t("work.remote.cloneIntro")}</p>
      <button
        type="button"
        disabled={busy}
        onClick={() => void start()}
        className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
      >
        <FolderDown size={13} />
        {t(cloning ? "work.remote.cloning" : "work.remote.clone")}
      </button>
    </div>
  );
}

function Branches({ data, compact }: { data: RemoteView; compact: boolean }) {
  const t = useT();
  const { busy, load, createBranch, deleteBranch } = useRemoteStore();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const defaultBranch = data.view.defaultBranch;

  const remove = async (branch: string) => {
    const ok = await ask(t("work.remote.deleteConfirm", { branch }), {
      title: t("work.remote.deleteTitle"),
      kind: "warning",
    }).catch(() => false);
    if (ok) await deleteBranch(branch);
  };

  return (
    <section className="space-y-1.5">
      <h3 className="flex h-6 items-center gap-2 text-xs font-semibold">
        {t("work.remote.branches")}
        <span className="font-normal text-text-subtle">{data.branches.length}</span>
        <button
          type="button"
          onClick={() => setAdding(!adding)}
          title={t("work.remote.newBranch")}
          className="ml-auto rounded p-1 font-normal text-text-subtle hover:bg-hover hover:text-text"
        >
          <Plus size={13} />
        </button>
      </h3>
      {adding && (
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            if (await createBranch(name, data.branch)) {
              setAdding(false);
              setName("");
            }
          }}
          className="space-y-1.5 rounded-lg border border-border bg-surface-raised p-2"
        >
          <input
            // biome-ignore lint/a11y/noAutofocus: opened on purpose
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("work.remote.branchName")}
            className={inputClass}
          />
          <div className="flex items-center gap-2 text-xs">
            <span className="min-w-0 flex-1 truncate text-text-muted">
              {t("work.remote.from", { branch: data.branch })}
            </span>
            <button
              type="button"
              onClick={() => setAdding(false)}
              className="rounded-md px-2 py-1 text-text-muted hover:bg-hover"
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
      )}
      <ul className={`space-y-px overflow-y-auto ${compact ? "max-h-48" : "max-h-64"}`}>
        {data.branches.map((b) => {
          const selected = b.name === data.branch;
          const isDefault = b.name === defaultBranch;
          return (
            <li key={b.name} className="group flex items-center gap-1">
              <button
                type="button"
                onClick={async () => {
                  if (b.name === data.branch) return;
                  const drafts = Object.keys(useRemoteStore.getState().drafts).length;
                  if (
                    drafts &&
                    !(await ask(t("work.files.switchConfirm", { n: drafts }), {
                      title: t("work.remote.branches"),
                      kind: "warning",
                    }).catch(() => false))
                  )
                    return;
                  void load(b.name);
                }}
                className={`flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1 text-left text-xs hover:bg-hover ${selected ? "bg-hover font-medium" : ""}`}
              >
                <GitBranch size={12} className="shrink-0 text-text-subtle" />
                <span className="min-w-0 flex-1 truncate">{b.name}</span>
                {isDefault && (
                  <span className="shrink-0 rounded-full bg-sunken px-1.5 py-px text-[10px] text-text-muted">
                    {t("work.remote.default")}
                  </span>
                )}
                {b.protected && (
                  <Shield
                    size={11}
                    className="shrink-0 text-text-subtle"
                    aria-label={t("work.remote.protected")}
                  />
                )}
              </button>
              {!isDefault && !b.protected && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void remove(b.name)}
                  title={t("work.remote.delete", { branch: b.name })}
                  className="rounded p-1 text-text-subtle opacity-0 hover:bg-hover hover:text-danger group-hover:opacity-100 disabled:opacity-30"
                >
                  <Trash2 size={12} />
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** The selected branch's PR and checks, or a form to open a PR from it. */
function BranchDetail({ data, compact }: { data: RemoteView; compact: boolean }) {
  const t = useT();
  const [creating, setCreating] = useState(false);
  const { view, branch } = data;
  const current = view.current;
  const isDefault = branch === view.defaultBranch;

  return (
    <div className="space-y-2 rounded-lg border border-border bg-surface-raised p-3">
      {current ? (
        <button
          type="button"
          onClick={() => open(current.url)}
          className="flex w-full items-center gap-2 text-left"
          title={`${current.head} → ${current.base}`}
        >
          <GitPullRequest size={15} className="shrink-0 text-text-subtle" />
          <span className="min-w-0 flex-1 truncate">
            <span className="text-text-subtle">#{current.number}</span> {current.title}
          </span>
          <PullBadge pull={current} />
          <ChecksSummary checks={view.checks} />
        </button>
      ) : (
        <div className="flex items-center gap-2">
          <GitPullRequest size={15} className="shrink-0 text-text-subtle" />
          <span className="min-w-0 flex-1 truncate text-text-muted">
            {isDefault ? t("work.github.defaultBranch", { branch }) : t("work.github.noPull")}
          </span>
          <ChecksSummary checks={view.checks} />
        </div>
      )}
      {!isDefault &&
        current?.state !== "open" &&
        (creating ? (
          <RemoteNewPull data={data} compact={compact} onDone={() => setCreating(false)} />
        ) : (
          <button
            type="button"
            onClick={() => setCreating(true)}
            className="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-fg hover:bg-primary-hover"
          >
            {t(current ? "work.github.newPull" : "work.github.createPull")}
          </button>
        ))}
    </div>
  );
}

function RemoteNewPull({
  data,
  compact,
  onDone,
}: {
  data: RemoteView;
  compact: boolean;
  onDone: () => void;
}) {
  const t = useT();
  const createPr = useRemoteStore((s) => s.createPr);
  const [title, setTitle] = useState(data.commits[0]?.subject ?? "");
  const [body, setBody] = useState("");
  const [base, setBase] = useState(data.view.defaultBranch);
  const [draft, setDraft] = useState(false);
  const [busy, setBusy] = useState(false);
  const bases = data.branches.map((b) => b.name).filter((n) => n !== data.branch);

  return (
    <form
      onSubmit={async (e) => {
        e.preventDefault();
        setBusy(true);
        const pull = await createPr({ head: data.branch, base, title, body, draft });
        setBusy(false);
        if (pull) {
          onDone();
          open(pull.url);
        }
      }}
      className="space-y-1.5"
    >
      <input
        // biome-ignore lint/a11y/noAutofocus: opened on purpose
        autoFocus
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder={t("work.github.pullTitle")}
        className={inputClass}
      />
      <textarea
        value={body}
        onChange={(e) => setBody(e.target.value)}
        placeholder={t("work.github.pullBody")}
        rows={compact ? 3 : 4}
        className={`${inputClass} resize-y`}
      />
      <div className="flex items-center gap-1.5 text-xs text-text-muted">
        <span className="shrink-0">{data.branch} →</span>
        <select
          aria-label={t("work.baseBranch")}
          value={base}
          onChange={(e) => setBase(e.target.value)}
          className={inputClass}
        >
          {bases.map((b) => (
            <option key={b} value={b}>
              {b}
            </option>
          ))}
        </select>
      </div>
      <div className="flex items-center gap-2 text-xs">
        <label className="flex items-center gap-1.5 text-text-muted">
          <input type="checkbox" checked={draft} onChange={(e) => setDraft(e.target.checked)} />
          {t("work.github.asDraft")}
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
          disabled={busy || !title.trim()}
          className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          {t(busy ? "work.github.creating" : "work.github.createPull")}
        </button>
      </div>
    </form>
  );
}

function Commits({ data }: { data: RemoteView }) {
  const t = useT();
  return (
    <Collapsible
      title={t("work.remote.commits", { branch: data.branch })}
      count={data.commits.length}
    >
      {data.commits.map((c) => (
        <li key={c.sha}>
          <button
            type="button"
            onClick={() => open(c.url)}
            title={`${c.sha} · @${c.author}`}
            className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs hover:bg-hover"
          >
            <GitCommitHorizontal size={13} className="shrink-0 text-text-subtle" />
            <span className="min-w-0 flex-1 truncate">{c.subject}</span>
            <span className="shrink-0 text-[11px] text-text-subtle">
              {timeAgo(seconds(c.date))}
            </span>
          </button>
        </li>
      ))}
    </Collapsible>
  );
}
