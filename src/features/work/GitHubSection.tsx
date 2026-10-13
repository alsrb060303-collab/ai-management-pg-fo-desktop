import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Check,
  ChevronDown,
  ChevronRight,
  CircleDot,
  Clock,
  ExternalLink,
  GitMerge,
  GitPullRequest,
  GitPullRequestClosed,
  GitPullRequestDraft,
  MessageSquare,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { useT } from "../../i18n";
import { type Checks, type Issue, type Pull, useGitHubStore } from "../../store/github";
import { useNotifyStore } from "../../store/notify";
import { useProjectStore } from "../../store/project";
import { useWorkStore } from "../../store/work";
import { Command, rich } from "./GitSetupGuide";
import { useWorkUi } from "./ui";

/** GitHub is polled far less often than local git (5,000 requests an hour). */
const POLL_MS = 60_000;

export const open = (url: string) => {
  if (isTauri()) void openUrl(url);
  else window.open(url, "_blank");
};

/**
 * Keeps the GitHub view of the open project current: on project, sign-in,
 * branch and push changes, and every minute while the window is shown.
 * Called once per window (main and sidebar), whatever page is shown.
 */
export function useGitHubSync() {
  const project = useProjectStore((s) => s.current);
  const overview = useWorkStore((s) => s.overview);
  const { status, init, load } = useGitHubStore();
  const signedIn = !!status?.account;
  const branch = overview?.head.branch ?? null;
  const commit = overview?.head.commit ?? null;
  const unpushed = overview?.unpushed ?? 0;

  useEffect(() => {
    void init();
  }, [init]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: branch, commit and unpushed are reload triggers
  useEffect(() => {
    void load(project);
  }, [project, signedIn, branch, commit, unpushed, load]);

  useEffect(() => {
    const tick = () => {
      // Hidden, keep watching an open PR so its merge or CI result is notified.
      const { view, project } = useGitHubStore.getState();
      const watching = useNotifyStore.getState().on && view?.current?.state === "open";
      if (document.visibilityState === "visible" || watching) void load(project);
    };
    const timer = setInterval(tick, POLL_MS);
    window.addEventListener("focus", tick);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", tick);
    };
  }, [load]);
}

/**
 * The work panel's GitHub part: sign-in, the checked-out branch's pull
 * request and checks (or a form to open one), and open PRs and issues.
 */
export function GitHubSection({ compact }: { compact: boolean }) {
  const t = useT();
  const overview = useWorkStore((s) => s.overview);
  const { status, view, loading, error } = useGitHubStore();
  const signedIn = !!status?.account;

  // No client ID in this build. Signed out, the sign-in shows for any
  // project; signed in, a project without a remote has nothing to show (the
  // setup guide covers adding one).
  if (!status?.configured || (signedIn && !overview?.hasRemote)) return null;

  return (
    <section className={`space-y-2 ${compact ? "text-xs" : "text-sm"}`}>
      <Header compact={compact} />
      {error && (
        <p className="selectable rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">
          {error}
        </p>
      )}
      {status.offline ? (
        <p className="text-xs text-text-subtle">{t("work.github.offline")}</p>
      ) : !signedIn ? (
        <SignIn />
      ) : view ? (
        <>
          <CurrentBranch compact={compact} />
          <PullList pulls={view.pulls} />
          <IssueList issues={view.issues} />
        </>
      ) : (
        !loading && <p className="text-xs text-text-subtle">{t("work.github.notGitHub")}</p>
      )}
    </section>
  );
}

function Header({ compact }: { compact: boolean }) {
  const t = useT();
  const { status, view, logout } = useGitHubStore();
  const account = status?.account;
  return (
    <h3 className="flex h-6 items-center gap-2 text-xs font-semibold">
      GitHub
      {view && account && (
        <button
          type="button"
          onClick={() => open(view.url)}
          className="flex min-w-0 items-center gap-1 truncate font-normal text-text-subtle hover:text-text"
          title={t("work.github.openOnGitHub")}
        >
          {view.slug}
          <ExternalLink size={11} className="shrink-0" />
        </button>
      )}
      {account && (
        <span className="ml-auto flex shrink-0 items-center gap-1.5 font-normal text-text-subtle">
          {!compact && `@${account.login}`}
          <button
            type="button"
            onClick={() => void logout()}
            title={t("work.github.signOutAccount", { login: account.login })}
            className="rounded px-1 py-0.5 hover:bg-hover hover:text-text"
          >
            {t("work.github.signOut")}
          </button>
        </span>
      )}
      {status?.offline && (
        <span className="ml-auto font-normal text-text-subtle">
          {t("work.github.disconnected")}
        </span>
      )}
    </h3>
  );
}

export function SignIn() {
  const t = useT();
  const { login, loginError, startLogin, cancelLogin } = useGitHubStore();
  const [copied, setCopied] = useState(false);

  if (!login) {
    return (
      <div className="space-y-2 rounded-lg border border-border bg-surface-raised p-3">
        <p className="text-text-muted">{t("work.github.signInIntro")}</p>
        {loginError && <p className="text-xs text-danger">{loginError}</p>}
        <button
          type="button"
          onClick={() => void startLogin()}
          className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-fg hover:bg-primary-hover"
        >
          {t("work.github.signIn")}
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-2.5 rounded-lg border border-border bg-surface-raised p-3">
      <p className="text-text-muted">{t("work.github.enterCode")}</p>
      <p className="selectable text-center font-mono text-2xl font-semibold tracking-[0.2em]">
        {login.userCode}
      </p>
      <button
        type="button"
        onClick={() => {
          void navigator.clipboard
            .writeText(login.userCode)
            .then(() => setCopied(true))
            .catch(() => {});
          open(login.verificationUri);
        }}
        className="flex w-full items-center justify-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-fg hover:bg-primary-hover"
      >
        {copied ? t("work.github.copied") : ""}
        {t("work.github.copyAndOpen")}
        <ExternalLink size={12} />
      </button>
      <div className="flex items-center gap-2 text-xs text-text-subtle">
        <Clock size={12} className="animate-pulse" />
        {t("work.github.waiting", { url: login.verificationUri })}
        <button
          type="button"
          onClick={cancelLogin}
          className="ml-auto rounded px-1.5 py-0.5 hover:bg-hover hover:text-text"
        >
          {t("work.cancel")}
        </button>
      </div>
    </div>
  );
}

/** Labels are `work.github.state.<state>`. */
/** GitHub's colors: open green, merged purple, closed red, draft grey. */
export function pullStyle(pull: Pick<Pull, "state" | "draft">) {
  if (pull.draft && pull.state === "open")
    return { key: "draft", Icon: GitPullRequestDraft, text: "text-text-muted", chip: "bg-sunken" };
  if (pull.state === "merged")
    return { key: "merged", Icon: GitMerge, text: "text-merged", chip: "bg-merged/15" };
  if (pull.state === "closed")
    return { key: "closed", Icon: GitPullRequestClosed, text: "text-danger", chip: "bg-danger/15" };
  return { key: "open", Icon: GitPullRequest, text: "text-success", chip: "bg-success/15" };
}

export function PullBadge({ pull, number }: { pull: Pull; number?: boolean }) {
  const t = useT();
  const s = pullStyle(pull);
  return (
    <span
      className={`flex shrink-0 items-center gap-1 rounded-full px-1.5 py-px text-[10px] font-medium ${s.chip} ${s.text}`}
    >
      <s.Icon size={11} />
      {number && `#${pull.number} `}
      {t(s.key === "draft" ? "work.github.draft" : `work.github.state.${pull.state}`)}
    </span>
  );
}

export function ChecksSummary({ checks }: { checks: Checks | null }) {
  const t = useT();
  if (!checks) return <span className="text-[11px] text-text-subtle">{t("work.github.noCi")}</span>;
  const parts: ReactNode[] = [];
  if (checks.failed)
    parts.push(
      <span key="f" className="flex items-center gap-0.5 text-danger">
        <X size={12} />
        {checks.failed}
      </span>,
    );
  if (checks.pending)
    parts.push(
      <span key="p" className="flex items-center gap-0.5 text-warning">
        <Clock size={11} />
        {checks.pending}
      </span>,
    );
  if (checks.passed)
    parts.push(
      <span key="s" className="flex items-center gap-0.5 text-success">
        <Check size={12} />
        {checks.passed}
      </span>,
    );
  const title = t("work.github.ciTitle", { ...checks });
  return (
    <span className="flex shrink-0 items-center gap-1.5 text-[11px]" title={title}>
      {parts.length ? (
        parts
      ) : (
        <span className="text-text-subtle">{t("work.github.skipped", { n: checks.skipped })}</span>
      )}
    </span>
  );
}

function CurrentBranch({ compact }: { compact: boolean }) {
  const t = useT();
  const view = useGitHubStore((s) => s.view);
  const overview = useWorkStore((s) => s.overview);
  const creating = useWorkUi((s) => s.creatingPull);
  const setCreating = useWorkUi((s) => s.setCreatingPull);
  if (!view || !overview) return null;
  const branch = overview.head.branch;
  if (!branch) return null;
  const current = view.current;

  if (!view.pushed && !current) {
    if (branch === view.defaultBranch) return null;
    return (
      <div className="space-y-1.5 rounded-lg border border-border bg-surface-raised p-3">
        <p className="text-text-muted">
          {rich(t("work.github.notPushed"), { branch: <b className="text-text">{branch}</b> })}
        </p>
        <Command lines={[`git push -u origin ${branch}`]} />
      </div>
    );
  }

  return (
    <div className="space-y-2 rounded-lg border border-border bg-surface-raised p-3">
      {current ? (
        <button
          type="button"
          onClick={() => open(current.url)}
          className="flex w-full items-center gap-2 text-left"
          title={`${current.head} → ${current.base}`}
        >
          {(() => {
            const { Icon, text } = pullStyle(current);
            return <Icon size={15} className={`shrink-0 ${text}`} />;
          })()}
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
            {branch === view.defaultBranch
              ? t("work.github.defaultBranch", { branch })
              : t("work.github.noPull")}
          </span>
          <ChecksSummary checks={view.checks} />
        </div>
      )}
      {branch !== view.defaultBranch &&
        current?.state !== "open" &&
        (creating ? (
          <NewPull compact={compact} onDone={() => setCreating(false)} />
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

export const inputClass =
  "w-full min-w-0 rounded-md border border-border bg-surface px-2 py-1 text-xs outline-none placeholder:text-text-subtle focus:border-border-strong";

function NewPull({ compact, onDone }: { compact: boolean; onDone: () => void }) {
  const t = useT();
  const view = useGitHubStore((s) => s.view);
  const createPr = useGitHubStore((s) => s.createPr);
  const overview = useWorkStore((s) => s.overview);
  const branch = overview?.head.branch ?? "";
  const subject = overview?.branches.find((b) => b.name === branch)?.subject ?? "";
  const [title, setTitle] = useState(subject);
  const [body, setBody] = useState("");
  const [base, setBase] = useState(view?.defaultBranch ?? "main");
  const [draft, setDraft] = useState(false);
  const [busy, setBusy] = useState(false);
  if (!view || !overview) return null;
  const bases = [
    view.defaultBranch,
    ...overview.branches.map((b) => b.name).filter((n) => n !== view.defaultBranch && n !== branch),
  ];
  const ahead = overview.head.ahead;

  return (
    <form
      onSubmit={async (e) => {
        e.preventDefault();
        setBusy(true);
        const pull = await createPr({ base, title, body, draft });
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
        <span className="shrink-0">{branch} →</span>
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
      {ahead > 0 && (
        <p className="text-[11px] text-warning">
          {rich(t(ahead === 1 ? "work.github.unpushed.one" : "work.github.unpushed"), {
            n: ahead,
            push: <code>git push</code>,
          })}
        </p>
      )}
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

export function Collapsible({
  title,
  count,
  children,
}: {
  title: string;
  count: number;
  children: ReactNode;
}) {
  const t = useT();
  const [shown, setShown] = useState(false);
  return (
    <div>
      <button
        type="button"
        onClick={() => setShown(!shown)}
        className="flex items-center gap-1 rounded px-1 py-0.5 text-xs text-text-muted hover:bg-hover hover:text-text"
      >
        {shown ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        {title} <span className="text-text-subtle">{count}</span>
      </button>
      {shown &&
        (count === 0 ? (
          <p className="px-2 py-1 text-[11px] text-text-subtle">{t("work.github.none")}</p>
        ) : (
          <ul className="mt-0.5 space-y-px">{children}</ul>
        ))}
    </div>
  );
}

function Row({
  url,
  icon,
  number,
  title,
  aside,
  tooltip,
}: {
  url: string;
  icon: ReactNode;
  number: number;
  title: string;
  aside?: ReactNode;
  tooltip: string;
}) {
  return (
    <li>
      <button
        type="button"
        onClick={() => open(url)}
        title={tooltip}
        className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs hover:bg-hover"
      >
        {icon}
        <span className="min-w-0 flex-1 truncate">
          <span className="text-text-subtle">#{number}</span> {title}
        </span>
        {aside}
      </button>
    </li>
  );
}

export function PullList({ pulls }: { pulls: Pull[] }) {
  const t = useT();
  return (
    <Collapsible title={t("work.github.openPulls")} count={pulls.length}>
      {pulls.map((p) => {
        const { Icon, text } = pullStyle(p);
        return (
          <Row
            key={p.number}
            url={p.url}
            icon={<Icon size={13} className={`shrink-0 ${text}`} />}
            number={p.number}
            title={p.title}
            tooltip={`${p.head} → ${p.base} · @${p.author}`}
            aside={p.draft ? <PullBadge pull={p} /> : null}
          />
        );
      })}
    </Collapsible>
  );
}

export function IssueList({ issues }: { issues: Issue[] }) {
  const t = useT();
  return (
    <Collapsible title={t("work.github.openIssues")} count={issues.length}>
      {issues.map((i) => (
        <Row
          key={i.number}
          url={i.url}
          icon={<CircleDot size={13} className="shrink-0 text-success" />}
          number={i.number}
          title={i.title}
          tooltip={`@${i.author}${i.labels.length ? ` · ${i.labels.join(", ")}` : ""}`}
          aside={
            i.comments > 0 ? (
              <span className="flex shrink-0 items-center gap-0.5 text-[11px] text-text-subtle">
                <MessageSquare size={11} />
                {i.comments}
              </span>
            ) : null
          }
        />
      ))}
    </Collapsible>
  );
}
