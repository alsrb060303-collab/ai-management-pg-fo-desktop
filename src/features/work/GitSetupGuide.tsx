import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, ChevronDown, ChevronRight, Copy, ExternalLink } from "lucide-react";
import { Fragment, type ReactNode, useState } from "react";
import { useT } from "../../i18n";
import { getPref, setPref } from "../../store/prefs";

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
    return (JSON.parse(getPref(DISMISS_KEY) ?? "[]") as string[]).includes(root);
  } catch {
    return false;
  }
}

function dismiss(root: string) {
  try {
    const list = JSON.parse(getPref(DISMISS_KEY) ?? "[]") as string[];
    setPref(DISMISS_KEY, JSON.stringify([...new Set([...list, root])]));
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
  const t = useT();
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

  const url = `https://github.com/${user.trim() || t("work.setup.userToken")}/${repo.trim() || t("work.setup.repoToken")}.git`;
  const steps: { title: string; body: ReactNode; optional?: boolean }[] = [];

  if (gitMissing) {
    steps.push({
      title: t("work.setup.installGit"),
      body: (
        <>
          <p>{t("work.setup.installGit.body")}</p>
          {isWindows ? (
            <Command lines={["winget install --id Git.Git -e --source winget"]} />
          ) : (
            <Command lines={["sudo apt install git"]} />
          )}
          <LinkButton href="https://git-scm.com/downloads">
            {t("work.setup.installGit.download")}
          </LinkButton>
          <p className="text-text-subtle">{t("work.setup.installGit.restart")}</p>
        </>
      ),
    });
  }

  if (!hasRemote) {
    steps.push({
      title: t("work.setup.createRepo"),
      optional: true,
      body: (
        <>
          <p>
            {rich(t("work.setup.createRepo.body"), {
              noInit: <b>{t("work.setup.createRepo.noInit")}</b>,
            })}
          </p>
          <LinkButton href="https://github.com/new">{t("work.setup.createRepo.open")}</LinkButton>
          <div className="grid grid-cols-[auto_1fr] items-center gap-x-2 gap-y-1">
            <label htmlFor="setup-user">{t("work.setup.createRepo.user")}</label>
            <input
              id="setup-user"
              value={user}
              onChange={(e) => update({ user: e.target.value })}
              placeholder={t("work.setup.createRepo.userPlaceholder")}
              className={inputClass}
            />
            <label htmlFor="setup-repo">{t("work.setup.createRepo.repo")}</label>
            <input
              id="setup-repo"
              value={repo}
              onChange={(e) => update({ repo: e.target.value })}
              className={inputClass}
            />
          </div>
          <p className="text-text-subtle">{t("work.setup.createRepo.hint")}</p>
        </>
      ),
    });
  }

  steps.push({
    title: t("work.setup.cd"),
    body: (
      <>
        <p>{t(isWindows ? "work.setup.cd.bodyWindows" : "work.setup.cd.body")}</p>
        <Command lines={[`cd "${project}"`]} />
      </>
    ),
  });

  if (stage === "none") {
    steps.push({
      title: t("work.setup.identity"),
      optional: true,
      body: (
        <>
          <p>{t("work.setup.identity.body")}</p>
          <Command
            lines={[
              `git config --global user.name "${t("work.setup.identity.name")}"`,
              'git config --global user.email "me@example.com"',
            ]}
          />
          <p className="text-text-subtle">{t("work.setup.identity.noreply")}</p>
        </>
      ),
    });
    steps.push({
      title: t("work.setup.init"),
      body: (
        <>
          <Command lines={["git init -b main"]} />
          <p className="text-text-subtle">{t("work.setup.init.hint")}</p>
        </>
      ),
    });
  }

  if (stage !== "noRemote") {
    steps.push({
      title: t("work.setup.firstCommit"),
      body: (
        <>
          <p>
            {rich(t("work.setup.firstCommit.body"), {
              env: <code>.env</code>,
              modules: <code>node_modules</code>,
              gitignore: <code>.gitignore</code>,
              status: <code>git status</code>,
            })}
          </p>
          <Command
            lines={[
              "git status",
              "git add .",
              `git commit -m "${t("work.setup.firstCommit.message")}"`,
            ]}
          />
        </>
      ),
    });
  }

  steps.push({
    title: t("work.setup.push"),
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
          {t(isWindows ? "work.setup.push.hintWindows" : "work.setup.push.hint")}
        </p>
      </>
    ),
  });

  const title = t(`work.setup.title.${stage}`);

  const list = (
    <ol className="space-y-3">
      {steps.map((step, i) => (
        <li key={step.title} className="flex gap-2.5">
          <span className="mt-px flex size-5 shrink-0 items-center justify-center rounded-full bg-accent-soft text-fs-11 font-semibold text-accent">
            {i + 1}
          </span>
          <div className="min-w-0 flex-1 space-y-1.5">
            <h4 className="font-semibold text-text">
              {step.title}
              {step.optional && (
                <span className="ml-1.5 font-normal text-text-subtle">
                  {t("work.setup.optional")}
                </span>
              )}
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
            className="shrink-0 rounded px-1.5 py-0.5 text-fs-11 text-text-subtle hover:bg-hover hover:text-text"
          >
            {t("work.setup.localOnly")}
          </button>
        )}
      </div>
      {open && <div className="border-t border-border px-3 py-3">{list}</div>}
    </section>
  );
}

const inputClass =
  "min-w-0 rounded-md border border-border bg-surface px-2 py-1 text-xs text-text outline-none placeholder:text-text-subtle focus:border-border-strong";

export function LinkButton({ href, children }: { href: string; children: ReactNode }) {
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
export function Command({ lines }: { lines: string[] }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  return (
    <div className="group relative rounded-md border border-border bg-sunken">
      <pre className="selectable overflow-x-auto py-1.5 pr-9 pl-2.5 font-mono text-fs-11.5 leading-relaxed text-text">
        {lines.join("\n")}
      </pre>
      <button
        type="button"
        aria-label={t("work.setup.copyCommands")}
        title={t("work.setup.copy")}
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

/** Fills the `{name}` slots of a translated string with elements. */
export function rich(template: string, values: Record<string, ReactNode>): ReactNode[] {
  return template.split(/\{(\w+)\}/).map((part, i) =>
    i % 2 ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: the parts never reorder
      <Fragment key={i}>{part in values ? values[part] : `{${part}}`}</Fragment>
    ) : (
      part
    ),
  );
}
