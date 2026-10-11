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
import { useProjectStore } from "../../store/project";
import {
  type Branch,
  type Change,
  type ChangeKind,
  type Diff,
  useWorkStore,
  type Worktree,
} from "../../store/work";

/** How often the state is re-read while the panel is visible. */
const POLL_MS = 4000;
/** Branches shown before "더 보기". */
const BRANCH_LIMIT = 12;

const confirm = (message: string, title: string, danger = false) =>
  ask(message, { title, kind: danger ? "warning" : "info" }).catch(() => false);

/**
 * The open project's branch, changed files, branches and worktrees. Shared by
 * the sidebar (`compact`) and the main window's work page.
 */
export function WorkPanel({ compact = false }: { compact?: boolean }) {
  const project = useProjectStore((s) => s.current);
  const choose = useProjectStore((s) => s.choose);
  const { overview, notGit, loading, error, load, refresh, dismissError } = useWorkStore();

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

  if (!project) {
    return (
      <div className="space-y-3 text-sm">
        <p className="text-text-muted">프로젝트 폴더를 고르면 브랜치와 worktree를 여기서 다뤄요.</p>
        <button
          type="button"
          onClick={() => void choose()}
          className="rounded-lg border border-border bg-surface-raised px-3 py-1.5 text-sm hover:border-border-strong"
        >
          폴더 선택
        </button>
      </div>
    );
  }

  return (
    <div className={compact ? "space-y-4" : "space-y-6"}>
      {error && (
        <div className="flex items-start gap-2 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">
          <p className="selectable min-w-0 flex-1 whitespace-pre-wrap break-words">{error}</p>
          <button type="button" aria-label="닫기" onClick={dismissError} className="shrink-0">
            <X size={13} />
          </button>
        </div>
      )}
      {loading && !overview && !notGit && <p className="text-xs text-text-muted">불러오는 중…</p>}
      {notGit && (
        <p className="text-sm text-text-muted">
          git 저장소가 아니에요. 터미널에서 <code className="font-mono">git init</code> 하면
          브랜치와 worktree를 다룰 수 있어요.
        </p>
      )}
      {overview && (
        <>
          <HeadBar compact={compact} />
          <Changes compact={compact} />
          <Branches compact={compact} />
          <Worktrees compact={compact} />
        </>
      )}
    </div>
  );
}

const OPERATIONS: Record<string, string> = {
  rebase: "리베이스 중",
  merge: "병합 중",
  "cherry-pick": "cherry-pick 중",
  revert: "되돌리기 중",
  bisect: "bisect 중",
};

function HeadBar({ compact }: { compact: boolean }) {
  const { overview, busy, refresh } = useWorkStore();
  if (!overview) return null;
  const { head, operation } = overview;
  return (
    <div className="flex items-center gap-2">
      <GitBranch size={compact ? 15 : 17} className="shrink-0 text-text-subtle" />
      <span className={`min-w-0 truncate font-semibold ${compact ? "text-sm" : "text-base"}`}>
        {head.branch ?? `분리된 HEAD ${head.commit ?? ""}`}
      </span>
      <Track ahead={head.ahead} behind={head.behind} />
      {operation && (
        <span className="shrink-0 rounded-full bg-warning/15 px-2 py-px text-[11px] text-warning">
          {OPERATIONS[operation] ?? operation}
        </span>
      )}
      {!head.commit && <span className="text-xs text-text-subtle">커밋 없음</span>}
      <IconButton
        label="새로 고침"
        disabled={busy}
        onClick={() => void refresh(true)}
        className="ml-auto size-7"
      >
        <RefreshCw size={14} />
      </IconButton>
    </div>
  );
}

function Track({ ahead, behind, gone }: { ahead: number; behind: number; gone?: boolean }) {
  if (gone) return <span className="shrink-0 text-[11px] text-text-subtle">원격 삭제됨</span>;
  if (!ahead && !behind) return null;
  return (
    <span
      className="flex shrink-0 items-center gap-1 text-[11px] text-text-muted"
      title={`원격보다 ${ahead}개 앞, ${behind}개 뒤`}
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

const KIND: Record<ChangeKind, { letter: string; label: string; color: string }> = {
  modified: { letter: "M", label: "수정", color: "text-warning" },
  added: { letter: "A", label: "추가", color: "text-success" },
  deleted: { letter: "D", label: "삭제", color: "text-danger" },
  renamed: { letter: "R", label: "이름 변경", color: "text-info" },
  copied: { letter: "C", label: "복사", color: "text-info" },
  typeChanged: { letter: "T", label: "종류 변경", color: "text-warning" },
  untracked: { letter: "U", label: "새 파일", color: "text-success" },
  conflicted: { letter: "!", label: "충돌", color: "text-danger" },
};

function Changes({ compact }: { compact: boolean }) {
  const { overview, selected, diff, select } = useWorkStore();
  if (!overview) return null;
  const { changes, changeCount } = overview;
  return (
    <Section title="변경 사항" count={changeCount}>
      {changeCount === 0 ? (
        <p className="text-xs text-text-subtle">커밋하지 않은 변경이 없어요.</p>
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
              외 {changeCount - changes.length}개
            </li>
          )}
        </ul>
      )}
    </Section>
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
  const kind = KIND[change.kind];
  const trimmed = change.path.replace(/\/$/, "");
  const slash = trimmed.lastIndexOf("/");
  const dir = slash >= 0 ? trimmed.slice(0, slash + 1) : "";
  const name = trimmed.slice(slash + 1) + (change.path.endsWith("/") ? "/" : "");
  const stage = change.staged ? (change.unstaged ? "일부 스테이징됨" : "스테이징됨") : "";
  return (
    <button
      type="button"
      onClick={onClick}
      title={`${kind.label}${stage ? ` · ${stage}` : ""}${change.origPath ? ` · ${change.origPath}에서` : ""}\n${change.path}`}
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
          aria-label="스테이징됨"
        />
      )}
    </button>
  );
}

function DiffView({ diff, compact }: { diff: Diff | null; compact: boolean }) {
  if (!diff) return <p className="px-2 py-2 text-[11px] text-text-subtle">diff 불러오는 중…</p>;
  if (diff.binary)
    return (
      <p className="px-2 py-2 text-[11px] text-text-subtle">바이너리 파일이라 표시하지 않아요.</p>
    );
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
        {lines.length === 0 && <div className="px-2 text-text-subtle">내용 변경 없음</div>}
      </pre>
      {diff.truncated && (
        <p className="border-t border-border px-2 py-1 text-[11px] text-text-subtle">
          너무 길어서 일부만 보여요.
        </p>
      )}
    </div>
  );
}

function timeAgo(seconds: number): string {
  if (!seconds) return "";
  const diff = Date.now() / 1000 - seconds;
  if (diff < 60) return "방금";
  if (diff < 3600) return `${Math.floor(diff / 60)}분 전`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}시간 전`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)}일 전`;
  return new Date(seconds * 1000).toLocaleDateString("ko-KR");
}

function Branches({ compact }: { compact: boolean }) {
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
          `'${b.name}' 브랜치를 삭제할까요? 커밋은 모두 다른 곳에 남아 있어요.`,
          "브랜치 삭제",
        )
      )
        await run("work_delete_branch", { name: b.name, force: false });
      return;
    }
    const first = await confirm(
      `'${b.name}' 브랜치에만 있는 커밋이 ${unmerged}개 있어요. 삭제하면 이 커밋을 잃어요.\n그래도 삭제할까요?`,
      "병합되지 않은 브랜치",
      true,
    );
    if (!first) return;
    const second = await confirm(
      `정말 '${b.name}'을(를) 강제로 삭제할까요? 되돌릴 수 없어요.`,
      "강제 삭제",
      true,
    );
    if (second) await run("work_delete_branch", { name: b.name, force: true });
  };

  return (
    <Section
      title="브랜치"
      count={branches.length}
      action={
        <IconButton
          label="새 브랜치"
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
            title={`${b.commit} ${b.subject}${b.upstream ? `\n원격: ${b.upstream}` : ""}${
              b.worktree && !b.current ? `\nworktree: ${b.worktree}` : ""
            }`}
          >
            <span className={`min-w-0 truncate ${b.current ? "font-semibold" : ""}`}>{b.name}</span>
            {b.current && <Badge>현재</Badge>}
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
                  전환
                </button>
                <IconButton
                  label={`${b.name} 삭제`}
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
          {all ? "접기" : `${branches.length - BRANCH_LIMIT}개 더 보기`}
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
        placeholder="새 브랜치 이름 (예: feature/login)"
        className={`${inputClass} w-full`}
      />
      <div className="flex items-center gap-1.5 text-xs text-text-muted">
        <span className="shrink-0">기준</span>
        <select
          aria-label="기준 브랜치"
          value={base}
          onChange={(e) => setBase(e.target.value)}
          className={inputClass}
        >
          {!current && <option value="">현재 HEAD</option>}
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
          만든 뒤 전환
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
          disabled={busy || !name.trim()}
          className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          만들기
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
          `'${name}' worktree 폴더를 삭제할까요? 브랜치 ${wt.branch}는 그대로 남아요.`,
          "worktree 삭제",
        )
      )
        await run("work_remove_worktree", { path: wt.path, force: false });
      return;
    }
    const what = [
      wt.changes
        ? `커밋하지 않은 변경 ${wt.changes}개`
        : wt.changes === null
          ? "커밋하지 않은 변경"
          : "",
      wt.branch ? "" : "어느 브랜치에도 없는 커밋",
    ]
      .filter(Boolean)
      .join(", ");
    const first = await confirm(
      `'${name}'에 ${what}이(가) 있을 수 있어요. 삭제하면 잃어요.\n그래도 삭제할까요?`,
      "변경이 남은 worktree",
      true,
    );
    if (!first) return;
    const second = await confirm(
      `정말 '${name}'을(를) 강제로 삭제할까요? 되돌릴 수 없어요.`,
      "강제 삭제",
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
              title="폴더가 사라진 worktree를 목록에서 지워요"
              className="mr-1 rounded px-1.5 py-0.5 text-[11px] font-normal text-text-muted hover:bg-hover"
            >
              정리
            </button>
          )}
          <IconButton
            label="새 worktree"
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
              {wt.isMain ? "기본" : folderName(wt.path)}
            </span>
            {wt.isCurrent && <Badge>열림</Badge>}
            {wt.managed && <Badge>Claude Code</Badge>}
            {wt.locked && <Badge>잠김</Badge>}
            {wt.prunable && <Badge>폴더 없음</Badge>}
            <span className="min-w-0 flex-1 truncate text-[11px] text-text-subtle">
              {wt.branch ?? (wt.commit ? `분리된 HEAD ${wt.commit}` : "")}
            </span>
            {wt.changes !== null && wt.changes > 0 && (
              <span className="shrink-0 text-[11px] text-warning group-hover:hidden">
                변경 {wt.changes}
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
                    열기
                  </button>
                )}
                <IconButton
                  label="폴더 보기"
                  onClick={() => void revealItemInDir(wt.path).catch(() => {})}
                  className="size-5"
                >
                  <FolderOpen size={12} />
                </IconButton>
                {!wt.isMain && !wt.isCurrent && (
                  <IconButton
                    label={`${folderName(wt.path)} 삭제`}
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
        placeholder="이름 (영문·숫자·-·_)"
        className={`${inputClass} w-full font-mono`}
      />
      <p className="text-[11px] text-text-subtle">
        {name && !valid
          ? "영문·숫자·-·_·. 만 쓸 수 있어요."
          : `.claude/worktrees/${name || "<이름>"} 에 브랜치 worktree-${name || "<이름>"} 로 만들어요.`}
      </p>
      <div className="flex items-center gap-1.5 text-xs text-text-muted">
        <span className="shrink-0">기준</span>
        <select
          aria-label="기준 브랜치"
          value={base}
          onChange={(e) => setBase(e.target.value)}
          className={inputClass}
        >
          {!current && <option value="">현재 HEAD</option>}
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
          취소
        </button>
        <button
          type="submit"
          disabled={busy || !valid}
          className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          만들기
        </button>
      </div>
    </form>
  );
}
