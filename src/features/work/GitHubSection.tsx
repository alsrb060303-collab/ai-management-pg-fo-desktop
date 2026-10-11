import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Check,
  ChevronDown,
  ChevronRight,
  CircleDot,
  Clock,
  ExternalLink,
  GitPullRequest,
  MessageSquare,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { type Checks, type Pull, useGitHubStore } from "../../store/github";
import { useProjectStore } from "../../store/project";
import { useWorkStore } from "../../store/work";
import { Command } from "./GitSetupGuide";

/** GitHub is polled far less often than local git (5,000 requests an hour). */
const POLL_MS = 60_000;

const open = (url: string) => {
  if (isTauri()) void openUrl(url);
  else window.open(url, "_blank");
};

/**
 * The work panel's GitHub part: sign-in, the checked-out branch's pull
 * request and checks (or a form to open one), and open PRs and issues.
 */
export function GitHubSection({ compact }: { compact: boolean }) {
  const project = useProjectStore((s) => s.current);
  const overview = useWorkStore((s) => s.overview);
  const { status, view, loading, error, init, load } = useGitHubStore();
  const signedIn = !!status?.account;
  const branch = overview?.head.branch ?? null;
  const commit = overview?.head.commit ?? null;
  const ahead = overview?.head.ahead ?? 0;

  useEffect(() => {
    void init();
  }, [init]);

  // Reload when the project, sign-in, branch or pushed state changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: branch, commit and ahead are reload triggers
  useEffect(() => {
    void load(project);
  }, [project, signedIn, branch, commit, ahead, load]);

  useEffect(() => {
    const tick = () => {
      if (document.visibilityState === "visible") void load(useGitHubStore.getState().project);
    };
    const timer = setInterval(tick, POLL_MS);
    window.addEventListener("focus", tick);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", tick);
    };
  }, [load]);

  // No client ID in this build, or the project has no remote (the setup
  // guide covers that).
  if (!status?.configured || !overview?.hasRemote) return null;

  return (
    <section className={`space-y-2 ${compact ? "text-xs" : "text-sm"}`}>
      <Header compact={compact} />
      {error && (
        <p className="selectable rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">
          {error}
        </p>
      )}
      {!signedIn ? (
        <SignIn />
      ) : view ? (
        <>
          <CurrentBranch compact={compact} />
          <PullList pulls={view.pulls} />
          <IssueList />
        </>
      ) : (
        !loading && (
          <p className="text-xs text-text-subtle">
            github.com 저장소가 아니라서 PR·이슈를 보여줄 수 없어요.
          </p>
        )
      )}
    </section>
  );
}

function Header({ compact }: { compact: boolean }) {
  const { status, view, logout } = useGitHubStore();
  const account = status?.account;
  return (
    <h3 className="flex h-6 items-center gap-2 text-xs font-semibold">
      GitHub
      {view && (
        <button
          type="button"
          onClick={() => open(view.url)}
          className="flex min-w-0 items-center gap-1 truncate font-normal text-text-subtle hover:text-text"
          title="GitHub에서 열기"
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
            title={`@${account.login} 로그아웃`}
            className="rounded px-1 py-0.5 hover:bg-hover hover:text-text"
          >
            로그아웃
          </button>
        </span>
      )}
      {status?.offline && <span className="ml-auto font-normal text-text-subtle">연결 안 됨</span>}
    </h3>
  );
}

function SignIn() {
  const { login, loginError, startLogin, cancelLogin } = useGitHubStore();
  const [copied, setCopied] = useState(false);

  if (!login) {
    return (
      <div className="space-y-2 rounded-lg border border-border bg-surface-raised p-3">
        <p className="text-text-muted">
          GitHub에 로그인하면 이 브랜치의 PR·CI 상태와 열린 PR·이슈를 보고, PR을 만들 수 있어요.
        </p>
        {loginError && <p className="text-xs text-danger">{loginError}</p>}
        <button
          type="button"
          onClick={() => void startLogin()}
          className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-fg hover:bg-primary-hover"
        >
          GitHub 로그인
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-2.5 rounded-lg border border-border bg-surface-raised p-3">
      <p className="text-text-muted">브라우저에서 GitHub에 아래 코드를 입력하면 로그인돼요.</p>
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
        {copied ? "복사됨 · " : ""}코드 복사하고 GitHub 열기
        <ExternalLink size={12} />
      </button>
      <div className="flex items-center gap-2 text-xs text-text-subtle">
        <Clock size={12} className="animate-pulse" />
        기다리는 중… ({login.verificationUri})
        <button
          type="button"
          onClick={cancelLogin}
          className="ml-auto rounded px-1.5 py-0.5 hover:bg-hover hover:text-text"
        >
          취소
        </button>
      </div>
    </div>
  );
}

const STATE: Record<Pull["state"], { label: string; className: string }> = {
  open: { label: "열림", className: "bg-success/15 text-success" },
  merged: { label: "병합됨", className: "bg-info/15 text-info" },
  closed: { label: "닫힘", className: "bg-danger/15 text-danger" },
};

function PullBadge({ pull }: { pull: Pull }) {
  const s =
    pull.draft && pull.state === "open"
      ? { label: "초안", className: "bg-sunken text-text-muted" }
      : STATE[pull.state];
  return (
    <span className={`shrink-0 rounded-full px-1.5 py-px text-[10px] ${s.className}`}>
      {s.label}
    </span>
  );
}

function ChecksSummary({ checks }: { checks: Checks | null }) {
  if (!checks) return <span className="text-[11px] text-text-subtle">CI 없음</span>;
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
  const title = `CI: 성공 ${checks.passed}, 실패 ${checks.failed}, 진행 중 ${checks.pending}, 건너뜀 ${checks.skipped}`;
  return (
    <span className="flex shrink-0 items-center gap-1.5 text-[11px]" title={title}>
      {parts.length ? parts : <span className="text-text-subtle">건너뜀 {checks.skipped}</span>}
    </span>
  );
}

function CurrentBranch({ compact }: { compact: boolean }) {
  const view = useGitHubStore((s) => s.view);
  const overview = useWorkStore((s) => s.overview);
  const [creating, setCreating] = useState(false);
  if (!view || !overview) return null;
  const branch = overview.head.branch;
  if (!branch) return null;
  const current = view.current;

  if (!view.pushed && !current) {
    if (branch === view.defaultBranch) return null;
    return (
      <div className="space-y-1.5 rounded-lg border border-border bg-surface-raised p-3">
        <p className="text-text-muted">
          <b className="text-text">{branch}</b> 브랜치가 아직 GitHub에 없어요. PR을 만들려면 먼저
          올려 주세요.
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
            {branch === view.defaultBranch ? `${branch} (기본 브랜치)` : "이 브랜치의 PR이 없어요"}
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
            {current ? "새 PR 만들기" : "PR 만들기"}
          </button>
        ))}
    </div>
  );
}

const inputClass =
  "w-full min-w-0 rounded-md border border-border bg-surface px-2 py-1 text-xs outline-none placeholder:text-text-subtle focus:border-border-strong";

function NewPull({ compact, onDone }: { compact: boolean; onDone: () => void }) {
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
        placeholder="PR 제목"
        className={inputClass}
      />
      <textarea
        value={body}
        onChange={(e) => setBody(e.target.value)}
        placeholder="설명 (선택, 마크다운)"
        rows={compact ? 3 : 4}
        className={`${inputClass} resize-y`}
      />
      <div className="flex items-center gap-1.5 text-xs text-text-muted">
        <span className="shrink-0">{branch} →</span>
        <select
          aria-label="기준 브랜치"
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
          아직 push 안 한 커밋 {ahead}개는 PR에 들어가지 않아요. <code>git push</code> 후 만들면
          같이 들어가요.
        </p>
      )}
      <div className="flex items-center gap-2 text-xs">
        <label className="flex items-center gap-1.5 text-text-muted">
          <input type="checkbox" checked={draft} onChange={(e) => setDraft(e.target.checked)} />
          초안으로
        </label>
        <button
          type="button"
          onClick={onDone}
          className="ml-auto rounded-md px-2 py-1 text-text-muted hover:bg-hover"
        >
          취소
        </button>
        <button
          type="submit"
          disabled={busy || !title.trim()}
          className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          {busy ? "만드는 중…" : "PR 만들기"}
        </button>
      </div>
    </form>
  );
}

function Collapsible({
  title,
  count,
  children,
}: {
  title: string;
  count: number;
  children: ReactNode;
}) {
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
          <p className="px-2 py-1 text-[11px] text-text-subtle">없어요.</p>
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

function PullList({ pulls }: { pulls: Pull[] }) {
  return (
    <Collapsible title="열린 PR" count={pulls.length}>
      {pulls.map((p) => (
        <Row
          key={p.number}
          url={p.url}
          icon={<GitPullRequest size={13} className="shrink-0 text-success" />}
          number={p.number}
          title={p.title}
          tooltip={`${p.head} → ${p.base} · @${p.author}`}
          aside={p.draft ? <PullBadge pull={p} /> : null}
        />
      ))}
    </Collapsible>
  );
}

function IssueList() {
  const issues = useGitHubStore((s) => s.view?.issues ?? []);
  return (
    <Collapsible title="열린 이슈" count={issues.length}>
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
