import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { diffLines } from "diff";
import { ChevronDown, ChevronRight, FolderOpen, GitBranch, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, IconButton, Segmented } from "../../components/ui";
import { type MessageKey, useT } from "../../i18n";
import type { ComposerDoc } from "../../store/composer";
import { type Pull, useGitHubStore } from "../../store/github";
import { useProjectStore } from "../../store/project";
import { type RemoteView, useRemoteStore } from "../../store/remote";

// Mirrors `PlanItem` / `Status` in src-tauri/src/composer/write.rs.
type Status =
  | { kind: "new" | "update" | "unchanged" | "modified" | "exists" | "commands" }
  | { kind: "blocked"; reason: string };

interface PlanItem {
  path: string;
  status: Status;
  personal: boolean;
  old: string | null;
  new: string;
  oldHash: string | null;
  note: string | null;
}

interface WriteResult {
  written: string[];
  skipped: [string, string][];
  warnings: [string, string][];
}

// Mirrors `RemotePlan` / `RemoteWriteResult` in src-tauri/src/composer/remote.rs.
interface RemotePlan {
  head: string;
  items: PlanItem[];
}

interface RemoteResult {
  commit: string | null;
  branch: string;
  written: string[];
  skipped: [string, string][];
}

type Mode = "local" | "github";
type CommitTo = "pr" | "direct";

const MODE_KEY = "composer.write.mode";
const REPO_KEY = "composer.write.repo";

function remembered(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Only a convenience.
  }
}

/** A branch name from the document's name, e.g. `ai-management/my-rules`. */
function branchFor(name: string): string {
  const slug = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
  return `ai-management/${slug || "docs"}`;
}

const STATUS: Record<Status["kind"], { label: MessageKey; className: string }> = {
  new: { label: "composer.write.status.new", className: "text-success" },
  update: { label: "composer.write.status.update", className: "text-info" },
  unchanged: { label: "composer.write.status.unchanged", className: "text-text-subtle" },
  modified: { label: "composer.write.status.modified", className: "text-warning" },
  exists: { label: "composer.write.status.exists", className: "text-warning" },
  commands: { label: "composer.write.status.commands", className: "text-warning" },
  blocked: { label: "composer.write.status.blocked", className: "text-danger" },
};

const needsConfirm = (s: Status) =>
  s.kind === "modified" || s.kind === "exists" || s.kind === "commands";

export function WriteDialog({ doc, onClose }: { doc: ComposerDoc; onClose: () => void }) {
  const t = useT();
  const [mode, setMode] = useState<Mode>(() =>
    remembered(MODE_KEY) === "github" ? "github" : "local",
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-6"
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("composer.write.title")}
        className="flex max-h-full w-full max-w-2xl flex-col rounded-2xl border border-border bg-surface-raised shadow-card"
      >
        <header className="flex items-center gap-3 border-b border-border px-5 py-3.5">
          <h2 className="font-serif text-lg font-medium">{t("composer.write.title")}</h2>
          <Segmented
            label={t("composer.write.where")}
            value={mode}
            options={[
              { value: "local", label: t("composer.write.mode.local") },
              { value: "github", label: t("composer.write.mode.github") },
            ]}
            onChange={(m) => {
              setMode(m);
              remember(MODE_KEY, m);
            }}
          />
          <IconButton label={t("composer.write.close")} onClick={onClose} className="ml-auto">
            <X size={17} />
          </IconButton>
        </header>
        {mode === "local" ? (
          <LocalWrite doc={doc} onClose={onClose} />
        ) : (
          <GitHubWrite doc={doc} onClose={onClose} />
        )}
      </div>
    </div>
  );
}

function LocalWrite({ doc, onClose }: { doc: ComposerDoc; onClose: () => void }) {
  const { current, choose, refresh } = useProjectStore();
  const [plan, setPlan] = useState<PlanItem[] | null>(null);
  const [confirmed, setConfirmed] = useState<Set<string>>(new Set());
  const [open, setOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<WriteResult | null>(null);
  const t = useT();

  const load = useCallback(async () => {
    if (!current) return;
    setPlan(null);
    setError(null);
    // Confirmations belong to the files shown for one project.
    setConfirmed(new Set());
    setOpen(null);
    try {
      setPlan(await invoke<PlanItem[]>("composer_plan", { project: current, doc }));
    } catch (e) {
      setError(String(e));
    }
  }, [current, doc]);

  useEffect(() => {
    void load();
  }, [load]);

  const writable = writableOf(plan, confirmed);

  const write = async () => {
    if (!current) return;
    setBusy(true);
    setError(null);
    try {
      // Each confirmation is tied to the content shown, so a file that changed
      // since the preview is not overwritten.
      const confirmations = confirmationsOf(plan, confirmed);
      const res = await invoke<WriteResult>("composer_write", {
        project: current,
        doc,
        confirmed: confirmations,
      });
      setResult(res);
      void refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="flex items-center gap-2 border-b border-border px-5 py-3 text-sm">
        <FolderOpen size={15} className="shrink-0 text-text-subtle" />
        <span className="selectable min-w-0 flex-1 truncate" title={current ?? undefined}>
          {current ?? t("composer.write.pickProject")}
        </span>
        <Button size="sm" onClick={() => void choose()}>
          {current ? t("composer.write.change") : t("composer.write.chooseFolder")}
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">
        {error && (
          <p className="mb-3 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">{error}</p>
        )}
        {result ? (
          <Result result={result} />
        ) : !current ? null : !plan ? (
          <p className="text-sm text-text-muted">{t("composer.write.checking")}</p>
        ) : (
          <PlanList
            plan={plan}
            confirmed={confirmed}
            setConfirmed={setConfirmed}
            open={open}
            setOpen={setOpen}
          />
        )}
      </div>

      <footer className="flex items-center gap-2 border-t border-border px-5 py-3">
        {result ? (
          <Button variant="primary" className="ml-auto" onClick={onClose}>
            {t("composer.write.close")}
          </Button>
        ) : (
          <>
            <span className="text-xs text-text-subtle">{t("composer.write.hint")}</span>
            <Button variant="ghost" className="ml-auto" onClick={onClose}>
              {t("composer.write.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={!current || busy || writable.length === 0}
              onClick={() => void write()}
            >
              {busy
                ? t("composer.write.writing")
                : t("composer.write.writeFiles", { n: writable.length })}
            </Button>
          </>
        )}
      </footer>
    </>
  );
}

function PlanList({
  plan,
  confirmed,
  setConfirmed,
  open,
  setOpen,
}: {
  plan: PlanItem[];
  confirmed: Set<string>;
  setConfirmed: (next: Set<string>) => void;
  open: string | null;
  setOpen: (path: string | null) => void;
}) {
  const t = useT();
  return (
    <ul className="space-y-1.5">
      {plan.map((item) => {
        const status = STATUS[item.status.kind];
        const expanded = open === item.path;
        return (
          <li key={item.path} className="rounded-lg border border-border">
            <div className="flex items-center gap-2 px-3 py-2">
              <button
                type="button"
                aria-label={
                  expanded ? t("composer.write.hideChanges") : t("composer.write.showChanges")
                }
                onClick={() => setOpen(expanded ? null : item.path)}
                className="text-text-subtle hover:text-text"
              >
                {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
              </button>
              <span className="min-w-0 flex-1 truncate font-mono text-[12px]">{item.path}</span>
              {item.personal && (
                <span className="rounded bg-sunken px-1 text-[10px] text-text-subtle">
                  {t("composer.personal")}
                </span>
              )}
              <span className={`shrink-0 text-xs font-medium ${status.className}`}>
                {t(status.label)}
              </span>
            </div>
            {item.note && <p className="px-9 pb-2 text-xs text-info">{item.note}</p>}
            {item.status.kind === "blocked" && (
              <p className="px-9 pb-2 text-xs text-danger">{item.status.reason}</p>
            )}
            {needsConfirm(item.status) && (
              <label className="flex items-center gap-2 px-9 pb-2 text-xs text-text-muted">
                <input
                  type="checkbox"
                  checked={confirmed.has(item.path)}
                  onChange={(e) => {
                    const next = new Set(confirmed);
                    if (e.target.checked) next.add(item.path);
                    else next.delete(item.path);
                    setConfirmed(next);
                  }}
                />
                {item.status.kind === "modified"
                  ? t("composer.write.overwriteModified")
                  : item.status.kind === "commands"
                    ? t("composer.write.confirmCommands")
                    : t("composer.write.overwriteForeign")}
              </label>
            )}
            {expanded && <Diff old={item.old ?? ""} next={item.new} />}
          </li>
        );
      })}
    </ul>
  );
}

const writableOf = (plan: PlanItem[] | null, confirmed: Set<string>) =>
  plan?.filter(
    (p) =>
      p.status.kind === "new" ||
      p.status.kind === "update" ||
      (needsConfirm(p.status) && confirmed.has(p.path)),
  ) ?? [];

const confirmationsOf = (plan: PlanItem[] | null, confirmed: Set<string>) =>
  (plan ?? [])
    .filter((p) => confirmed.has(p.path))
    .map((p) => ({ path: p.path, oldHash: p.oldHash }));

const field =
  "h-8 min-w-0 rounded-lg border border-border bg-surface px-2 text-sm focus:border-accent focus:outline-none";

/** Writes straight to a GitHub branch: one commit, optionally on a new branch with a PR. */
function GitHubWrite({ doc, onClose }: { doc: ComposerDoc; onClose: () => void }) {
  const t = useT();
  const account = useGitHubStore((s) => s.status?.account ?? null);
  const initGitHub = useGitHubStore((s) => s.init);
  const repos = useRemoteStore((s) => s.repos);
  const reposLoading = useRemoteStore((s) => s.reposLoading);
  const loadRepos = useRemoteStore((s) => s.loadRepos);

  const [slug, setSlug] = useState<string | null>(() => remembered(REPO_KEY));
  const [view, setView] = useState<RemoteView | null>(null);
  const [branch, setBranch] = useState<string | null>(null);
  const [plan, setPlan] = useState<RemotePlan | null>(null);
  const [confirmed, setConfirmed] = useState<Set<string>>(new Set());
  const [open, setOpen] = useState<string | null>(null);
  const [commitTo, setCommitTo] = useState<CommitTo>("pr");
  const [newBranch, setNewBranch] = useState(() => branchFor(doc.name));
  const [message, setMessage] = useState(() =>
    t("composer.write.gh.defaultMessage", { name: doc.name }),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<{
    res: RemoteResult;
    pull: Pull | null;
    slug: string;
  } | null>(null);

  useEffect(() => {
    void initGitHub();
  }, [initGitHub]);

  useEffect(() => {
    if (account) void loadRepos();
  }, [account, loadRepos]);

  // Branches of the chosen repository; the default branch first.
  useEffect(() => {
    if (!slug || !account) return;
    let live = true;
    setView(null);
    setBranch(null);
    setError(null);
    invoke<RemoteView>("github_remote", { slug, branch: null })
      .then((v) => {
        if (!live) return;
        setView(v);
        setBranch(v.branch);
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [slug, account]);

  const load = useCallback(async () => {
    if (!slug || !branch) return;
    setPlan(null);
    setError(null);
    setConfirmed(new Set());
    setOpen(null);
    try {
      setPlan(await invoke<RemotePlan>("composer_plan_remote", { slug, branch, doc }));
    } catch (e) {
      setError(String(e));
    }
  }, [slug, branch, doc]);

  useEffect(() => {
    void load();
  }, [load]);

  const writable = writableOf(plan?.items ?? null, confirmed);
  const target = commitTo === "pr" ? newBranch.trim() : branch;
  const branchTaken =
    commitTo === "pr" && !!view?.branches.some((b) => b.name === newBranch.trim());

  const write = async () => {
    if (!slug || !branch || !plan) return;
    setBusy(true);
    setError(null);
    try {
      const res = await invoke<RemoteResult>("composer_write_remote", {
        slug,
        branch,
        head: plan.head,
        doc,
        confirmed: confirmationsOf(plan.items, confirmed),
        message,
        newBranch: commitTo === "pr" ? newBranch.trim() : null,
      });
      let pull: Pull | null = null;
      if (res.commit && commitTo === "pr") {
        try {
          pull = await invoke<Pull>("github_remote_create_pr", {
            slug,
            head: res.branch,
            base: branch,
            title: message.split("\n")[0],
            body: t("composer.write.gh.prBody", { name: doc.name }),
            draft: false,
          });
        } catch (e) {
          setError(t("composer.write.gh.prFailed", { reason: String(e) }));
        }
      }
      setResult({ res, pull, slug });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!account) {
    return <div className="px-5 py-6 text-sm text-text-muted">{t("composer.write.gh.signIn")}</div>;
  }

  return (
    <>
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-5 py-3 text-sm">
        <GitBranch size={15} className="shrink-0 text-text-subtle" />
        <select
          aria-label={t("composer.write.gh.repo")}
          className={`${field} flex-1`}
          value={slug ?? ""}
          onChange={(e) => {
            const next = e.target.value || null;
            setSlug(next);
            if (next) remember(REPO_KEY, next);
          }}
        >
          <option value="">
            {reposLoading ? t("composer.write.checking") : t("composer.write.gh.pickRepo")}
          </option>
          {slug && !repos?.some((r) => r.slug === slug) && <option value={slug}>{slug}</option>}
          {repos?.map((r) => (
            <option key={r.slug} value={r.slug}>
              {r.slug}
            </option>
          ))}
        </select>
        <select
          aria-label={t("composer.write.gh.branch")}
          className={`${field} w-40`}
          value={branch ?? ""}
          disabled={!view}
          onChange={(e) => setBranch(e.target.value)}
        >
          {view?.branches.map((b) => (
            <option key={b.name} value={b.name}>
              {b.name}
            </option>
          ))}
        </select>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">
        {error && (
          <p className="mb-3 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">{error}</p>
        )}
        {result ? (
          <RemoteDone result={result} />
        ) : !slug ? null : !plan ? (
          !error && <p className="text-sm text-text-muted">{t("composer.write.checking")}</p>
        ) : (
          <>
            <p className="mb-2 text-xs text-text-muted">{t("composer.write.gh.hint")}</p>
            <PlanList
              plan={plan.items}
              confirmed={confirmed}
              setConfirmed={setConfirmed}
              open={open}
              setOpen={setOpen}
            />
          </>
        )}
      </div>

      {!result && plan && (
        <div className="space-y-2 border-t border-border px-5 py-3 text-sm">
          <div className="flex flex-wrap items-center gap-3">
            <label className="flex items-center gap-1.5">
              <input
                type="radio"
                name="commit-to"
                checked={commitTo === "pr"}
                onChange={() => setCommitTo("pr")}
              />
              {t("composer.write.gh.toPr")}
            </label>
            <label className="flex items-center gap-1.5">
              <input
                type="radio"
                name="commit-to"
                checked={commitTo === "direct"}
                onChange={() => setCommitTo("direct")}
              />
              {t("composer.write.gh.toBranch", { branch: branch ?? "" })}
            </label>
          </div>
          {commitTo === "pr" && (
            <input
              aria-label={t("composer.write.gh.newBranch")}
              className={`${field} w-full font-mono`}
              value={newBranch}
              onChange={(e) => setNewBranch(e.target.value)}
            />
          )}
          {branchTaken && (
            <p className="text-xs text-warning">{t("composer.write.gh.branchTaken")}</p>
          )}
          <input
            aria-label={t("composer.write.gh.message")}
            className={`${field} w-full`}
            value={message}
            onChange={(e) => setMessage(e.target.value)}
          />
        </div>
      )}

      <footer className="flex items-center gap-2 border-t border-border px-5 py-3">
        {result ? (
          <Button variant="primary" className="ml-auto" onClick={onClose}>
            {t("composer.write.close")}
          </Button>
        ) : (
          <>
            <Button variant="ghost" className="ml-auto" onClick={onClose}>
              {t("composer.write.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={
                busy || !plan || writable.length === 0 || !target || branchTaken || !message.trim()
              }
              onClick={() => void write()}
            >
              {busy
                ? t("composer.write.writing")
                : t("composer.write.gh.commit", { n: writable.length })}
            </Button>
          </>
        )}
      </footer>
    </>
  );
}

function RemoteDone({
  result: { res, pull, slug },
}: {
  result: { res: RemoteResult; pull: Pull | null; slug: string };
}) {
  const t = useT();
  return (
    <div className="space-y-3 text-sm">
      <p>
        {res.commit
          ? t("composer.write.gh.committed", { n: res.written.length, branch: res.branch })
          : t("composer.write.wroteNone")}
      </p>
      {res.written.length > 0 && (
        <ul className="space-y-0.5 font-mono text-[12px] text-success">
          {res.written.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
      {res.skipped.length > 0 && (
        <ul className="space-y-0.5 text-xs text-warning">
          {res.skipped.map(([p, why]) => (
            <li key={p}>
              <span className="font-mono">{p}</span>: {why}
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap gap-2">
        {pull && (
          <Button size="sm" variant="primary" onClick={() => void openUrl(pull.url)}>
            {t("composer.write.gh.openPr", { n: pull.number })}
          </Button>
        )}
        {res.commit && (
          <Button
            size="sm"
            onClick={() => void openUrl(`https://github.com/${slug}/commit/${res.commit}`)}
          >
            {t("composer.write.gh.openCommit")}
          </Button>
        )}
      </div>
      <p className="text-xs text-text-subtle">{t("composer.write.gh.pullHint")}</p>
    </div>
  );
}

function Result({ result }: { result: WriteResult }) {
  const t = useT();
  return (
    <div className="space-y-3 text-sm">
      <p>
        {result.written.length > 0
          ? t("composer.write.wrote", { n: result.written.length })
          : t("composer.write.wroteNone")}
      </p>
      {result.written.length > 0 && (
        <ul className="space-y-0.5 font-mono text-[12px] text-success">
          {result.written.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
      {result.warnings.length > 0 && (
        <ul className="space-y-0.5 text-xs text-warning">
          {result.warnings.map(([p, why]) => (
            <li key={p}>
              <span className="font-mono">{p}</span>:{" "}
              {t("composer.write.gitExcludeFailed", { reason: why })}
            </li>
          ))}
        </ul>
      )}
      {result.skipped.length > 0 && (
        <ul className="space-y-0.5 text-xs text-warning">
          {result.skipped.map(([p, why]) => (
            <li key={p}>
              <span className="font-mono">{p}</span>: {why}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Diff({ old, next }: { old: string; next: string }) {
  const parts = diffLines(old, next);
  return (
    <pre className="selectable max-h-72 overflow-auto border-t border-border bg-sunken px-3 py-2 font-mono text-[11.5px] leading-[1.55]">
      {parts.map((part, i) => {
        const cls = part.added
          ? "bg-success/15 text-success"
          : part.removed
            ? "bg-danger-soft text-danger line-through decoration-danger/40"
            : "text-text-muted";
        const sign = part.added ? "+ " : part.removed ? "- " : "  ";
        const lines = part.value.replace(/\n$/, "").split("\n");
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: diff hunks have no identity besides position
          <span key={i} className={`block ${cls}`}>
            {lines.map((l) => `${sign}${l}`).join("\n")}
          </span>
        );
      })}
    </pre>
  );
}
