import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, ChevronDown, ChevronRight, Copy, ExternalLink } from "lucide-react";
import { type ReactNode, useState } from "react";

/** Where the project is in getting set up. */
export type SetupStage =
  /** Not a git repository yet. */
  | "none"
  /** A repository without its first commit. */
  | "noCommit"
  /** Committed, but not connected to GitHub (or any remote). */
  | "noRemote";

const isWindows = navigator.userAgent.includes("Windows");

function folderName(path: string) {
  return (
    path
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() ?? ""
  );
}

/** A folder name as a GitHub repository name (letters, digits, `-_.`). */
function repoName(path: string) {
  return (
    folderName(path)
      .replace(/[^A-Za-z0-9._-]+/g, "-")
      .replace(/^-+|-+$/g, "") || "my-project"
  );
}

/** What was typed, per project, so it survives the guide changing stage. */
const typed = new Map<string, { user: string; repo: string }>();

const DISMISS_KEY = "gitSetupDismissed";

function dismissed(root: string): boolean {
  try {
    return (JSON.parse(localStorage.getItem(DISMISS_KEY) ?? "[]") as string[]).includes(root);
  } catch {
    return false;
  }
}

function dismiss(root: string) {
  try {
    const list = JSON.parse(localStorage.getItem(DISMISS_KEY) ?? "[]") as string[];
    localStorage.setItem(DISMISS_KEY, JSON.stringify([...new Set([...list, root])]));
  } catch {
    // Only a convenience; the card just shows again.
  }
}

/**
 * Step-by-step commands to put a project under git and on GitHub. Shown in
 * full for a folder outside git, and as a card with the remaining steps for a
 * repository without a first commit or a remote. The panel notices each step
 * on its next refresh, so the guide shrinks as the user goes.
 */
export function GitSetupGuide({
  stage,
  project,
  branch,
  hasRemote,
  gitMissing,
  compact,
}: {
  stage: SetupStage;
  project: string;
  /** Branch to push; "main" before the repository exists. */
  branch: string;
  /** A remote is set already (e.g. an empty repository was cloned). */
  hasRemote: boolean;
  gitMissing: boolean;
  compact: boolean;
}) {
  const [fields, setFields] = useState(
    () => typed.get(project) ?? { user: "", repo: repoName(project) },
  );
  const [open, setOpen] = useState(stage !== "noRemote");
  const [hidden, setHidden] = useState(() => stage === "noRemote" && dismissed(project));
  const { user, repo } = fields;
  const update = (next: Partial<typeof fields>) => {
    const value = { ...fields, ...next };
    typed.set(project, value);
    setFields(value);
  };

  if (hidden) return null;

  const url = `https://github.com/${user.trim() || "<GitHub-아이디>"}/${repo.trim() || "<저장소-이름>"}.git`;
  const steps: { title: string; body: ReactNode; optional?: boolean }[] = [];

  if (gitMissing) {
    steps.push({
      title: "Git 설치",
      body: (
        <>
          <p>이 컴퓨터에서 git을 찾지 못했어요. 터미널에서 설치하거나 공식 사이트에서 받아요.</p>
          {isWindows ? (
            <Command lines={["winget install --id Git.Git -e --source winget"]} />
          ) : (
            <Command lines={["sudo apt install git"]} />
          )}
          <LinkButton href="https://git-scm.com/downloads">git-scm.com에서 받기</LinkButton>
          <p className="text-text-subtle">
            설치한 뒤에는 이 앱과 터미널을 껐다 켜야 git이 인식돼요.
          </p>
        </>
      ),
    });
  }

  if (!hasRemote) {
    steps.push({
      title: "GitHub에서 저장소 만들기",
      optional: true,
      body: (
        <>
          <p>
            GitHub에 로그인한 뒤 새 저장소를 만들어요.{" "}
            <b>README · .gitignore · license는 체크하지 마세요.</b> 빈 저장소여야 아래 올리기 단계가
            바로 돼요.
          </p>
          <LinkButton href="https://github.com/new">github.com/new 열기</LinkButton>
          <div className="grid grid-cols-[auto_1fr] items-center gap-x-2 gap-y-1">
            <label htmlFor="setup-user">GitHub 아이디</label>
            <input
              id="setup-user"
              value={user}
              onChange={(e) => update({ user: e.target.value })}
              placeholder="예: octocat"
              className={inputClass}
            />
            <label htmlFor="setup-repo">저장소 이름</label>
            <input
              id="setup-repo"
              value={repo}
              onChange={(e) => update({ repo: e.target.value })}
              className={inputClass}
            />
          </div>
          <p className="text-text-subtle">
            입력하면 아래 명령어에 바로 채워져요. 로컬에서만 쓸 거면 건너뛰어도 돼요.
          </p>
        </>
      ),
    });
  }

  steps.push({
    title: "터미널에서 프로젝트 폴더로 이동",
    body: (
      <>
        <p>{isWindows ? "PowerShell이나 Windows 터미널" : "터미널"}을 열고:</p>
        <Command lines={[`cd "${project}"`]} />
      </>
    ),
  });

  if (stage === "none") {
    steps.push({
      title: "내 이름과 이메일 등록 (처음 한 번만)",
      optional: true,
      body: (
        <>
          <p>커밋에 남을 이름이에요. 이미 등록했으면 건너뛰어요.</p>
          <Command
            lines={[
              'git config --global user.name "내 이름"',
              'git config --global user.email "me@example.com"',
            ]}
          />
          <p className="text-text-subtle">
            이메일을 공개하기 싫으면 GitHub 설정 › Emails의 noreply 주소를 써요.
          </p>
        </>
      ),
    });
    steps.push({
      title: "git 저장소 만들기",
      body: (
        <>
          <Command lines={["git init -b main"]} />
          <p className="text-text-subtle">끝나면 이 탭이 몇 초 안에 알아서 바뀌어요.</p>
        </>
      ),
    });
  }

  if (stage !== "noRemote") {
    steps.push({
      title: "첫 커밋",
      body: (
        <>
          <p>
            올리면 안 되는 파일(<code>.env</code>, 비밀번호, <code>node_modules</code> 등)은 먼저{" "}
            <code>.gitignore</code>에 적어 두세요. <code>git status</code>로 올라갈 파일을 확인할 수
            있어요.
          </p>
          <Command lines={["git status", "git add .", 'git commit -m "첫 커밋"']} />
        </>
      ),
    });
  }

  steps.push({
    title: "GitHub에 올리기",
    optional: stage !== "noRemote",
    body: (
      <>
        <Command
          lines={[
            ...(hasRemote ? [] : [`git remote add origin ${url}`]),
            `git push -u origin ${branch}`,
          ]}
        />
        <p className="text-text-subtle">
          처음 올릴 때 GitHub 로그인 창이 뜨면 로그인하면 돼요
          {isWindows ? " (Git for Windows에 들어 있는 Git Credential Manager)" : ""}.
        </p>
      </>
    ),
  });

  const title =
    stage === "none"
      ? "git 저장소가 아니에요. 아래 순서대로 설정해 주세요."
      : stage === "noCommit"
        ? "아직 커밋이 없어요. 첫 커밋을 만들어 주세요."
        : "GitHub에 아직 연결하지 않았어요.";

  const list = (
    <ol className="space-y-3">
      {steps.map((step, i) => (
        <li key={step.title} className="flex gap-2.5">
          <span className="mt-px flex size-5 shrink-0 items-center justify-center rounded-full bg-accent-soft text-[11px] font-semibold text-accent">
            {i + 1}
          </span>
          <div className="min-w-0 flex-1 space-y-1.5">
            <h4 className="font-semibold text-text">
              {step.title}
              {step.optional && <span className="ml-1.5 font-normal text-text-subtle">(선택)</span>}
            </h4>
            {step.body}
          </div>
        </li>
      ))}
    </ol>
  );

  const text = compact ? "text-xs" : "text-sm";

  if (stage === "none") {
    return (
      <section className={`space-y-4 text-text-muted ${text}`}>
        <p className="text-text">{title}</p>
        {list}
      </section>
    );
  }

  return (
    <section
      className={`rounded-lg border border-border bg-surface-raised text-text-muted ${text}`}
    >
      <div className="flex items-center gap-1.5 px-3 py-2">
        <button
          type="button"
          onClick={() => setOpen(!open)}
          className="flex min-w-0 flex-1 items-center gap-1.5 text-left text-text"
        >
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          <span className="truncate">{title}</span>
        </button>
        {stage === "noRemote" && (
          <button
            type="button"
            onClick={() => {
              dismiss(project);
              setHidden(true);
            }}
            className="shrink-0 rounded px-1.5 py-0.5 text-[11px] text-text-subtle hover:bg-hover hover:text-text"
          >
            로컬에서만 쓸게요
          </button>
        )}
      </div>
      {open && <div className="border-t border-border px-3 py-3">{list}</div>}
    </section>
  );
}

const inputClass =
  "min-w-0 rounded-md border border-border bg-surface px-2 py-1 text-xs text-text outline-none placeholder:text-text-subtle focus:border-border-strong";

function LinkButton({ href, children }: { href: string; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={() => {
        if (isTauri()) void openUrl(href);
        else window.open(href, "_blank");
      }}
      className="flex items-center gap-1 text-accent hover:underline"
    >
      {children}
      <ExternalLink size={12} />
    </button>
  );
}

/** Commands to type, with a button that copies all of them. */
function Command({ lines }: { lines: string[] }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="group relative rounded-md border border-border bg-sunken">
      <pre className="selectable overflow-x-auto py-1.5 pr-9 pl-2.5 font-mono text-[11.5px] leading-relaxed text-text">
        {lines.join("\n")}
      </pre>
      <button
        type="button"
        aria-label="명령어 복사"
        title="복사"
        onClick={() => {
          void navigator.clipboard.writeText(lines.join("\n")).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          });
        }}
        className="absolute top-1 right-1 rounded p-1 text-text-subtle hover:bg-hover hover:text-text"
      >
        {copied ? <Check size={13} className="text-success" /> : <Copy size={13} />}
      </button>
    </div>
  );
}
